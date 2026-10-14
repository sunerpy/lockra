//! The browser extension's side of the core (crate::browser): what the channel asks for a paired
//! extension's session, every decision taken here, and the interface's commands (pairing,
//! confirmations, the policy). A browser request never restarts the idle timer: a busy extension
//! must not keep the vault open.

use std::time::Duration;

use lockra_autofill::{GeneratorError, OsRandom, PasswordRules, RandomSource, WebUrl, generate};
use tokio::sync::oneshot;
use tokio::time::{Instant, timeout_at};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::items::{copy_part_text, remember_generated};
use super::{Core, PhaseState, Session, State, current_code, touch_items};
use crate::browser::BrowserPolicy;
use crate::browser::{
    Activity, ActivityEvent, BrowserView, CONFIRM_TTL, ClientInfo, ConfirmAction, ConfirmReason, ConfirmView, CopyRequest, FillAnswer, FillField, FillRequest,
    FrameRule, GenerateAnswer, GenerateRequest, ItemSummary, MAX_FRAME_CHAIN, MAX_PENDING_PAIRS, OtpAnswer, OtpRequest, PAIR_TTL, PairOutcome, PairedExtension,
    PairedView, PendingConfirm, PendingPair, QueryAnswer, QueryPurpose, QueryRequest, Refusal, SEARCH_MAX, SaveAction, SaveAdvice, SaveRequest, SearchRequest,
    SessionId, SessionState, SiteRuleRequest, StaticKey, StatusAnswer, Trigger, UNLOCK_REQUEST_EVERY, UpdateRequest, choose, cross_origin, listed, matches,
    offered, page, site_of,
};
use crate::error::{CoreResult, ErrorCode};
use crate::item::{Matching, Protection, Purpose, SiteRuleKind, Tri, VaultItem};
use crate::logins::{CopyPart, ItemDraft, ItemPatch, SecretChange, UrlDraft};
use crate::settings::Settings;
use crate::ui::{CopiedPart, Notice, Platform};

/// How often a session's last connection is written to the vault.
const LAST_SEEN_EVERY_MS: u64 = 10 * 60 * 1000;

/// A request's addresses, read.
struct Frame {
    frame: WebUrl,
    top: WebUrl,
    chain: Vec<WebUrl>,
}

impl Frame {
    fn of(facts: &crate::browser::FrameFacts) -> Result<Self, Refusal> {
        if facts.frame_chain.len() > MAX_FRAME_CHAIN {
            return Err(Refusal::Invalid);
        }
        let chain = facts.frame_chain.iter().map(|u| page(u)).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { frame: page(&facts.frame_url)?, top: page(&facts.top_url)?, chain })
    }

    /// The form is in the page itself, not in a frame.
    fn top_level(&self) -> bool {
        self.chain.is_empty() && self.frame == self.top
    }

    fn cross_origin(&self) -> bool {
        cross_origin(&self.frame, &self.top, &self.chain)
    }
}

/// What a fill or a copy does once allowed.
enum Decision {
    /// Go ahead.
    Now,
    /// Ask the user first; the answer comes on `outcome` until `deadline`.
    Ask { id: Uuid, outcome: oneshot::Receiver<bool>, deadline: Instant },
}

/// Takes a waiting request out of the state when the request ends, however it ends (answered,
/// lapsed, or the channel gone).
struct Waiting<'a> {
    core: &'a Core,
    pair: Option<Uuid>,
    confirm: Option<Uuid>,
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        let removed = {
            let mut st = self.core.lock();
            let before = st.browser.pairing.len() + st.browser.confirms.len();
            if let Some(id) = self.pair {
                st.browser.pairing.retain(|p| p.id != id);
            }
            if let Some(id) = self.confirm {
                st.browser.confirms.retain(|c| c.view.id != id);
            }
            before != st.browser.pairing.len() + st.browser.confirms.len()
        };
        if removed {
            self.core.changed();
        }
    }
}

impl Core {
    /// This build offers the browser extension here: password management on, the desktop app.
    pub(super) fn browser_offered(&self) -> bool {
        self.shared.config.features.passwords && self.shared.config.platform != Platform::Android
    }

    /// The interface's browser commands: refused on the phone and while password management is off.
    fn browser_command(&self) -> CoreResult<()> {
        if !self.shared.config.features.passwords {
            return Err(ErrorCode::FeatureUnavailable.into());
        }
        if self.shared.config.platform == Platform::Android {
            return Err(ErrorCode::BrowserUnavailable.into());
        }
        Ok(())
    }

    /// The integration is on: the shell listens for the extensions and keeps their manifests.
    pub fn browser_enabled(&self) -> bool {
        self.browser_offered() && self.lock().settings.browser_integration
    }

    /// Turn the integration on or off (`settings.json`): the shell does it once it has written or
    /// removed the browsers' manifests. The pairings and the policy stay in the vault either way.
    pub fn browser_set_enabled(&self, on: bool) -> CoreResult<()> {
        self.browser_command()?;
        let settings = Settings { browser_integration: on, ..self.lock().settings.clone() };
        self.shared.settings_store.save(&settings)?;
        self.lock().settings.browser_integration = on;
        self.changed();
        Ok(())
    }

    /// The app's static Noise key, while unlocked; made and kept in the vault the first time. It
    /// stays in this process, in the channel's memory, and never goes to the webview.
    pub fn browser_static_secret(&self) -> Option<Zeroizing<[u8; 32]>> {
        if !self.browser_offered() {
            return None;
        }
        let mut st = self.lock();
        let PhaseState::Unlocked(session) = &mut st.phase else { return None };
        if let Some(key) = session.data.local.as_ref().and_then(|l| l.browser.key.as_ref()) {
            return Some(key.0.clone());
        }
        let key = random_key()?;
        session.data.local_mut().browser.key = Some(StaticKey(key.clone()));
        if let Err(error) = self.write_vault(session) {
            tracing::warn!(?error, "could not keep the browser channel's key");
            session.data.local_mut().browser.key = None;
            return None;
        }
        Some(key)
    }

    /// A paired extension's static key, for its session's handshake; none while locked or for a
    /// pairing that is gone.
    pub fn browser_peer(&self, pairing: Uuid) -> Option<[u8; 32]> {
        if !self.browser_offered() {
            return None;
        }
        let st = self.lock();
        let PhaseState::Unlocked(session) = &st.phase else { return None };
        paired(session, pairing).map(|p| p.public_key)
    }

    /// An extension asks Lockra to unlock: the window comes up with the unlock screen (or the
    /// biometric check), at most once every [`UNLOCK_REQUEST_EVERY`]. Whether it was passed on.
    pub fn browser_unlock_request(&self, client: &ClientInfo) -> bool {
        if !self.browser_offered() {
            return false;
        }
        let now = Instant::now();
        {
            let mut st = self.lock();
            if !matches!(st.phase, PhaseState::Locked) || st.browser.unlock_asked.is_some_and(|at| now.duration_since(at) < UNLOCK_REQUEST_EVERY) {
                return false;
            }
            st.browser.unlock_asked = Some(now);
        }
        self.notice(Notice::BrowserUnlockRequested { browser: client.clone().cleaned().browser });
        true
    }

    /// An extension finished the pairing handshake and shows `code`: the request waits for the user
    /// to compare the codes and approve, at most [`PAIR_TTL`]. The new pairing's id once approved.
    pub async fn browser_pair(&self, client: ClientInfo, public_key: [u8; 32], code: String) -> Result<Uuid, Refusal> {
        if !self.browser_offered() {
            return Err(Refusal::Unavailable);
        }
        let client = client.cleaned();
        let id = Uuid::new_v4();
        let (sender, outcome) = oneshot::channel();
        let deadline = Instant::now() + PAIR_TTL;
        {
            let mut st = self.lock();
            if !matches!(st.phase, PhaseState::Unlocked(_)) {
                return Err(Refusal::Locked);
            }
            let now = Instant::now();
            st.browser.pairing.retain(|p| p.expires_at > now);
            if st.browser.pairing.len() >= MAX_PENDING_PAIRS {
                return Err(Refusal::Busy);
            }
            let expires_at_ms = self.now_ms() + millis(PAIR_TTL);
            st.browser.pairing.push(PendingPair {
                id,
                client: client.clone(),
                public_key,
                code: code.clone(),
                expires_at: deadline,
                expires_at_ms,
                outcome: Some(sender),
            });
        }
        let _waiting = Waiting { core: self, pair: Some(id), confirm: None };
        self.changed();
        self.notice(Notice::BrowserPairRequested { id, browser: client.browser.clone(), code });
        match timeout_at(deadline, outcome).await {
            Ok(Ok(PairOutcome::Approved(pairing))) => Ok(pairing),
            Ok(Ok(PairOutcome::Denied)) => Err(Refusal::Denied),
            Ok(Err(_)) => Err(if matches!(self.lock().phase, PhaseState::Unlocked(_)) { Refusal::Denied } else { Refusal::Locked }),
            Err(_) => Err(Refusal::Expired),
        }
    }

    /// The user compared the codes and approves the pairing, with the master password or the
    /// biometric check that unlocks this vault.
    pub async fn browser_pair_approve(&self, id: Uuid, password: Option<Zeroizing<String>>, reason: Option<String>) -> CoreResult<()> {
        self.browser_command()?;
        {
            let st = self.lock();
            super::unlocked(&st)?;
            let now = Instant::now();
            if !st.browser.pairing.iter().any(|p| p.id == id && p.expires_at > now) {
                return Err(ErrorCode::BrowserPairExpired.into());
            }
        }
        self.confirm_presence(password, reason).await?;
        let now_ms = self.now_ms();
        {
            let mut guard = self.lock();
            let st = &mut *guard;
            let PhaseState::Unlocked(session) = &mut st.phase else { return Err(ErrorCode::Locked.into()) };
            let now = Instant::now();
            let Some(index) = st.browser.pairing.iter().position(|p| p.id == id && p.expires_at > now) else {
                return Err(ErrorCode::BrowserPairExpired.into());
            };
            let pending = &st.browser.pairing[index];
            let pairing = Uuid::new_v4();
            let extension = PairedExtension {
                id: pairing,
                browser: pending.client.browser.clone(),
                label: pending.client.label.clone(),
                extension: pending.client.extension.clone(),
                public_key: pending.public_key,
                paired_at_ms: now_ms,
                last_seen_ms: None,
            };
            let browser = &mut session.data.local_mut().browser;
            let before = browser.paired.clone();
            // The same extension paired again: its earlier pairing goes.
            browser.paired.retain(|p| p.public_key != extension.public_key);
            browser.paired.push(extension.clone());
            if let Err(error) = self.write_vault(session) {
                session.data.local_mut().browser.paired = before;
                return Err(error);
            }
            let mut pending = st.browser.pairing.remove(index);
            st.browser.record(Activity {
                at_ms: now_ms,
                browser: extension.browser,
                label: extension.label,
                site: String::new(),
                event: ActivityEvent::Paired,
                refusal: None,
                item_id: None,
                title: None,
            });
            if let Some(sender) = pending.outcome.take() {
                let _ = sender.send(PairOutcome::Approved(pairing));
            }
        }
        self.changed();
        Ok(())
    }

    /// The user refuses the pairing.
    pub fn browser_pair_deny(&self, id: Uuid) -> CoreResult<()> {
        self.browser_command()?;
        {
            let mut st = self.lock();
            let index = st.browser.pairing.iter().position(|p| p.id == id).ok_or(ErrorCode::BrowserPairExpired)?;
            let mut pending = st.browser.pairing.remove(index);
            if let Some(sender) = pending.outcome.take() {
                let _ = sender.send(PairOutcome::Denied);
            }
        }
        self.changed();
        Ok(())
    }

    /// Remove a pairing: its sessions end at once, and the extension has to pair again.
    pub fn browser_unpair(&self, id: Uuid) -> CoreResult<()> {
        self.browser_command()?;
        let now_ms = self.now_ms();
        {
            let mut guard = self.lock();
            let st = &mut *guard;
            let PhaseState::Unlocked(session) = &mut st.phase else { return Err(ErrorCode::Locked.into()) };
            let browser = &mut session.data.local_mut().browser;
            let index = browser.paired.iter().position(|p| p.id == id).ok_or(ErrorCode::BrowserNotPaired)?;
            let removed = browser.paired.remove(index);
            if let Err(error) = self.write_vault(session) {
                session.data.local_mut().browser.paired.insert(index, removed);
                return Err(error);
            }
            st.browser.sessions.retain(|_, s| s.pairing != id);
            st.browser.paused.remove(&id);
            st.browser.page_access.remove(&id);
            st.browser.confirms.retain(|c| st.browser.sessions.contains_key(&c.session));
            st.browser.record(Activity {
                at_ms: now_ms,
                browser: removed.browser,
                label: removed.label,
                site: String::new(),
                event: ActivityEvent::Unpaired,
                refusal: None,
                item_id: None,
                title: None,
            });
        }
        self.changed();
        Ok(())
    }

    /// A paired extension opened a session (its handshake succeeded with the pairing's key).
    pub fn browser_session_open(&self, pairing: Uuid, client: ClientInfo) -> Result<SessionId, Refusal> {
        if !self.browser_offered() {
            return Err(Refusal::Unavailable);
        }
        let client = client.cleaned();
        let now_ms = self.now_ms();
        let id = {
            let mut guard = self.lock();
            let st = &mut *guard;
            let PhaseState::Unlocked(session) = &mut st.phase else { return Err(Refusal::Locked) };
            let stale = {
                let extension = session.data.local_mut().browser.paired.iter_mut().find(|p| p.id == pairing).ok_or(Refusal::NotPaired)?;
                let stale = extension.last_seen_ms.is_none_or(|seen| now_ms.saturating_sub(seen) >= LAST_SEEN_EVERY_MS);
                if stale {
                    extension.last_seen_ms = Some(now_ms);
                }
                stale
            };
            // Recency only: a failed write loses nothing that matters.
            if stale && let Err(error) = self.write_vault(session) {
                tracing::warn!(?error, "could not record the extension's last connection");
            }
            st.browser.next_session += 1;
            let id = st.browser.next_session;
            st.browser.page_access.insert(pairing, client.page_access);
            st.browser.sessions.insert(id, SessionState::new(pairing, client));
            id
        };
        self.changed();
        Ok(SessionId(id))
    }

    /// The session ended (the extension disconnected, or the channel dropped it).
    pub fn browser_session_close(&self, session: SessionId) {
        let removed = {
            let mut st = self.lock();
            let removed = st.browser.sessions.remove(&session.0).is_some();
            st.browser.confirms.retain(|c| c.session != session.0);
            removed
        };
        if removed {
            self.changed();
        }
    }

    /// The session still stands: the vault is unlocked and the pairing still there. The channel
    /// asks after every change of the state, to tell the extension its pairing was removed.
    pub fn browser_session_alive(&self, session: SessionId) -> bool {
        let st = self.lock();
        let PhaseState::Unlocked(vault) = &st.phase else { return false };
        st.browser.sessions.get(&session.0).is_some_and(|s| paired(vault, s.pairing).is_some())
    }

    /// The session's state, for the popup (also while paused).
    pub fn browser_status(&self, session: SessionId) -> Result<StatusAnswer, Refusal> {
        let st = self.lock();
        let (vault, state) = active(&st, session, true)?;
        Ok(StatusAnswer {
            pairing: state.pairing,
            app_version: self.shared.config.app_version.clone(),
            policy: policy(vault),
            items: u32::try_from(vault.data.items.iter().filter(|i| offered(i)).count()).unwrap_or(u32::MAX),
            paused: st.browser.paused.contains(&state.pairing),
        })
    }

    /// The logins for a frame, without secrets; for a submitted form, whether to offer saving it.
    pub fn browser_query(&self, session: SessionId, request: QueryRequest) -> Result<QueryAnswer, Refusal> {
        let frame = Frame::of(&request.facts)?;
        let now = Instant::now();
        let mut guard = self.lock();
        let st = &mut *guard;
        let (vault, _) = active(st, session, false)?;
        let never_fill = listed(&vault.data, SiteRuleKind::NeverFill, &frame.frame) || listed(&vault.data, SiteRuleKind::NeverFill, &frame.top);
        let never_save = listed(&vault.data, SiteRuleKind::NeverSave, &frame.frame) || listed(&vault.data, SiteRuleKind::NeverSave, &frame.top);
        let (level, found) = matches(&vault.data, &frame.frame);
        let summaries: Vec<ItemSummary> =
            found.iter().filter_map(|(id, downgrade)| vault.data.items.iter().find(|i| i.id == *id).map(|i| summary(i, *downgrade, true))).collect();
        let save = (request.purpose == QueryPurpose::Save).then(|| advice(&vault.data, &found, never_save, &request.username, &request.password));
        let answer = QueryAnswer {
            site: site_of(&frame.frame),
            level: if never_fill { None } else { level },
            items: if never_fill { Vec::new() } else { summaries },
            never_fill,
            never_save,
            cross_origin: frame.cross_origin(),
            policy: policy(vault),
            save,
        };
        if !st.browser.sessions.get_mut(&session.0).is_some_and(|s| s.query(now)) {
            return Err(self.pause(st, session, &answer.site));
        }
        Ok(answer)
    }

    /// One login's secrets for a frame: the one named, or the likeliest match. A login that asks
    /// first, or a form in a frame of another site where the policy says to ask, waits for the
    /// user in Lockra (at most [`CONFIRM_TTL`]).
    pub async fn browser_fill(&self, session: SessionId, request: FillRequest) -> Result<FillAnswer, Refusal> {
        let frame = Frame::of(&request.facts)?;
        let site = site_of(&frame.frame);
        let (item_id, decision) = {
            let mut guard = self.lock();
            let st = &mut *guard;
            let decided = self.decide_fill(st, session, &request, &frame);
            match decided {
                Ok(decided) => decided,
                Err(refusal) => {
                    self.refused(st, session, &site, refusal, request.item_id);
                    drop(guard);
                    self.changed();
                    return Err(refusal);
                }
            }
        };
        self.wait(decision).await?;
        let answer = self.complete_fill(session, item_id, &frame, request.trigger)?;
        Ok(answer)
    }

    /// The code of a login filled on this site in the last few minutes, for a second sign-in step.
    pub fn browser_otp(&self, session: SessionId, request: OtpRequest) -> Result<OtpAnswer, Refusal> {
        let frame = Frame::of(&request.facts)?;
        let site = site_of(&frame.frame);
        let now = Instant::now();
        let now_ms = self.now_ms();
        let (answer, clear_after) = {
            let mut guard = self.lock();
            let st = &mut *guard;
            let (vault, state) = active(st, session, false)?;
            if listed(&vault.data, SiteRuleKind::NeverFill, &frame.frame) {
                return Err(Refusal::NeverFill);
            }
            if !state.filled_lately(request.item_id, &site, now) {
                return Err(Refusal::NoMatch);
            }
            let item = vault.data.items.iter().find(|i| i.id == request.item_id && offered(i)).ok_or(Refusal::NotFound)?;
            let policy = policy(vault);
            if !choose(&item.autofill.fill_otp, policy.fill_otp) {
                return Err(Refusal::Protected);
            }
            let entry = item.totp_entry.and_then(|e| vault.data.get(e)).ok_or(Refusal::NotFound)?;
            let code = current_code(entry, now_ms);
            let (title, copy) = (item.title.clone(), policy.copy_otp);
            if !st.browser.sessions.get_mut(&session.0).is_some_and(|s| s.secret(&site, now)) {
                return Err(self.pause(st, session, &site));
            }
            let clear_after = if copy { self.copy_secret(st, &code)? } else { None };
            self.record(st, session, &site, ActivityEvent::OtpFilled, Some(request.item_id), Some(title));
            (OtpAnswer { code, copied: copy }, clear_after)
        };
        self.changed();
        if answer.copied {
            self.notice(Notice::ItemCopied { item_id: Some(request.item_id), part: CopiedPart::Totp, clear_after_s: clear_after });
        }
        Ok(answer)
    }

    /// Put a part of a login on the clipboard (the popup's search, any site): a secret of a login
    /// that asks first waits for the user in Lockra.
    pub async fn browser_copy(&self, session: SessionId, request: CopyRequest) -> Result<(), Refusal> {
        let now = Instant::now();
        let decision = {
            let mut guard = self.lock();
            let st = &mut *guard;
            let (vault, state) = active(st, session, false)?;
            let item = vault.data.items.iter().find(|i| i.id == request.item_id && offered(i)).ok_or(Refusal::NotFound)?;
            let secret = match request.part {
                CopyPart::Password => true,
                CopyPart::Field { id } => item.field(id).is_some_and(|f| f.hidden),
                CopyPart::Username | CopyPart::Email | CopyPart::Totp => false,
            };
            let site = item.urls.iter().find_map(|u| WebUrl::parse_saved(&u.url)).map(|u| site_of(&u)).unwrap_or_default();
            let ask = secret && item.protection_level() == Protection::Confirm;
            let view = ask.then(|| ConfirmView {
                id: Uuid::new_v4(),
                item_id: item.id,
                title: item.title.clone(),
                username: username_of(item),
                frame_url: String::new(),
                top_url: None,
                browser: state.client.browser.clone(),
                label: state.client.label.clone(),
                reasons: vec![ConfirmReason::Protected],
                action: ConfirmAction::Copy,
                needs_proof: true,
                expires_at_ms: self.now_ms() + millis(CONFIRM_TTL),
            });
            if secret && !st.browser.sessions.get_mut(&session.0).is_some_and(|s| s.secret(&site, now)) {
                return Err(self.pause(st, session, &site));
            }
            match view {
                Some(view) => self.ask(st, session, view),
                None => Decision::Now,
            }
        };
        self.wait(decision).await?;
        let clear_after = {
            let mut guard = self.lock();
            let st = &mut *guard;
            active(st, session, false)?;
            let text = {
                let PhaseState::Unlocked(vault) = &st.phase else { return Err(Refusal::Locked) };
                copy_part_text(vault, request.item_id, request.part, self.now_ms()).map_err(|_| Refusal::NotFound)?
            };
            let clear_after = self.copy_secret(st, &text)?;
            let title = item_title(st, request.item_id);
            self.record(st, session, "", ActivityEvent::Copied, Some(request.item_id), title);
            clear_after
        };
        self.changed();
        let part = match request.part {
            CopyPart::Username => CopiedPart::Username,
            CopyPart::Email => CopiedPart::Email,
            CopyPart::Password => CopiedPart::Password,
            CopyPart::Totp => CopiedPart::Totp,
            CopyPart::Field { .. } => CopiedPart::Field,
        };
        self.notice(Notice::ItemCopied { item_id: Some(request.item_id), part, clear_after_s: clear_after });
        Ok(())
    }

    /// Save what a form submitted as a new login (the user chose to in the page).
    pub fn browser_save(&self, session: SessionId, request: SaveRequest) -> Result<Uuid, Refusal> {
        let frame = Frame::of(&request.facts)?;
        let site = site_of(&frame.frame);
        {
            let mut guard = self.lock();
            let st = &mut *guard;
            let (vault, _) = active(st, session, false)?;
            if listed(&vault.data, SiteRuleKind::NeverSave, &frame.frame) {
                return Err(Refusal::NeverSave);
            }
            if !st.browser.sessions.get_mut(&session.0).is_some_and(|s| s.query(Instant::now())) {
                return Err(self.pause(st, session, &site));
            }
        }
        let title = if request.title.trim().is_empty() { site.clone() } else { request.title.clone() };
        let draft = ItemDraft {
            title: title.clone(),
            username: request.username.clone(),
            password: SecretChange::Set { value: request.password.clone() },
            urls: vec![UrlDraft { url: origin(&frame.frame), matching: Matching::Domain }],
            group: request.group.clone(),
            ..ItemDraft::default()
        };
        let id = self.item_add(draft).map_err(|_| Refusal::Invalid)?;
        self.saved(session, &site, id, title, false);
        Ok(id)
    }

    /// Change a login's password to what a form submitted (the user chose to in the page): the old
    /// one goes into its history. The login must match the frame.
    pub fn browser_update(&self, session: SessionId, request: UpdateRequest) -> Result<(), Refusal> {
        let frame = Frame::of(&request.facts)?;
        let site = site_of(&frame.frame);
        let title = {
            let mut guard = self.lock();
            let st = &mut *guard;
            let (vault, _) = active(st, session, false)?;
            if listed(&vault.data, SiteRuleKind::NeverSave, &frame.frame) {
                return Err(Refusal::NeverSave);
            }
            let (_, found) = matches(&vault.data, &frame.frame);
            if !found.iter().any(|(id, _)| *id == request.item_id) {
                return Err(Refusal::NoMatch);
            }
            let title = vault.data.items.iter().find(|i| i.id == request.item_id).map(|i| i.title.clone()).unwrap_or_default();
            if !st.browser.sessions.get_mut(&session.0).is_some_and(|s| s.query(Instant::now())) {
                return Err(self.pause(st, session, &site));
            }
            title
        };
        self.item_update(request.item_id, ItemPatch { password: SecretChange::Set { value: request.password }, ..ItemPatch::default() })
            .map_err(|_| Refusal::Invalid)?;
        self.saved(session, &site, request.item_id, title, true);
        Ok(())
    }

    /// A password for a sign-up form, held to the page's rules (or the bundled ones of its site);
    /// saved at once as a new login when asked.
    pub fn browser_generate(&self, session: SessionId, request: GenerateRequest) -> Result<GenerateAnswer, Refusal> {
        let frame = Frame::of(&request.facts)?;
        let site = site_of(&frame.frame);
        let now_ms = self.now_ms();
        let (options, username) = {
            let mut guard = self.lock();
            let st = &mut *guard;
            let (vault, _) = active(st, session, false)?;
            if request.save.is_some() && listed(&vault.data, SiteRuleKind::NeverSave, &frame.frame) {
                return Err(Refusal::NeverSave);
            }
            let identity = vault.data.local.as_ref().map(|l| l.identity.clone()).unwrap_or_default();
            let default = if identity.username.is_empty() { identity.email } else { identity.username };
            let username = request.save.as_ref().and_then(|s| s.username.clone()).unwrap_or(default);
            if !st.browser.sessions.get_mut(&session.0).is_some_and(|s| s.query(Instant::now())) {
                return Err(self.pause(st, session, &site));
            }
            (st.settings.generator.clone(), username)
        };
        let rules = PasswordRules::for_page(frame.frame.host(), request.rules.as_deref(), request.minlength, request.maxlength);
        let made = generate(&options, rules.as_ref(), &mut OsRandom::new()).map_err(|error| match error {
            GeneratorError::RulesUnsatisfiable | GeneratorError::NoCharacters => Refusal::Invalid,
            GeneratorError::Random(_) => Refusal::Unavailable,
        })?;
        let password = made.password;
        {
            let mut guard = self.lock();
            let PhaseState::Unlocked(vault) = &mut guard.phase else { return Err(Refusal::Locked) };
            remember_generated(vault, password.clone(), Some(site.clone()), now_ms);
            let _ = self.write_vault(vault);
        }
        let item_id = match &request.save {
            Some(save) => {
                let title = if save.title.trim().is_empty() { site.clone() } else { save.title.clone() };
                let draft = ItemDraft {
                    title: title.clone(),
                    username: username.clone(),
                    password: SecretChange::Set { value: password.clone() },
                    urls: vec![UrlDraft { url: origin(&frame.frame), matching: Matching::Domain }],
                    ..ItemDraft::default()
                };
                let id = self.item_add(draft).map_err(|_| Refusal::Invalid)?;
                self.saved(session, &site, id, title, false);
                Some(id)
            }
            None => {
                let mut guard = self.lock();
                self.record(&mut guard, session, &site, ActivityEvent::Generated, None, None);
                drop(guard);
                self.changed();
                None
            }
        };
        Ok(GenerateAnswer { password, username, item_id })
    }

    /// "Not on this site": a never-save or never-fill rule for the frame's site.
    pub fn browser_site_rule(&self, session: SessionId, request: SiteRuleRequest) -> Result<(), Refusal> {
        let frame = Frame::of(&request.facts)?;
        let kind = match request.kind {
            Some(kind @ (SiteRuleKind::NeverSave | SiteRuleKind::NeverFill)) => kind,
            _ => return Err(Refusal::Invalid),
        };
        {
            let st = self.lock();
            active(&st, session, false)?;
        }
        self.site_rule_add(kind, &[site_of(&frame.frame)]).map_err(|_| Refusal::Invalid)
    }

    /// The popup's search over every login the extensions may see, at most [`SEARCH_MAX`]; those
    /// that match the tab's page say so (the others can only be copied).
    pub fn browser_search(&self, session: SessionId, request: SearchRequest) -> Result<Vec<ItemSummary>, Refusal> {
        let text = request.text.trim().to_lowercase();
        let top = WebUrl::parse(&request.top_url);
        let mut guard = self.lock();
        let st = &mut *guard;
        let (vault, _) = active(st, session, false)?;
        let matching: Vec<(Uuid, bool)> = top.as_ref().map(|top| matches(&vault.data, top).1).unwrap_or_default();
        let found: Vec<ItemSummary> = if text.is_empty() {
            Vec::new()
        } else {
            vault
                .data
                .items
                .iter()
                .filter(|i| offered(i) && searched(i, &text))
                .take(SEARCH_MAX)
                .map(|i| {
                    let hit = matching.iter().find(|(id, _)| *id == i.id);
                    summary(i, hit.is_some_and(|(_, d)| *d), hit.is_some())
                })
                .collect()
        };
        if !st.browser.sessions.get_mut(&session.0).is_some_and(|s| s.query(Instant::now())) {
            return Err(self.pause(st, session, ""));
        }
        Ok(found)
    }

    /// The user lets a waiting fill or copy go ahead: with the master password or the biometric
    /// check where the login asks for it, with a click where the frame rule only asks.
    pub async fn browser_confirm(&self, id: Uuid, password: Option<Zeroizing<String>>, reason: Option<String>) -> CoreResult<()> {
        self.browser_command()?;
        let needs_proof = {
            let st = self.lock();
            super::unlocked(&st)?;
            let now = Instant::now();
            st.browser.confirms.iter().find(|c| c.view.id == id && c.expires_at > now).ok_or(ErrorCode::BrowserPairExpired)?.view.needs_proof
        };
        if needs_proof {
            self.confirm_presence(password, reason).await?;
        }
        {
            let mut st = self.lock();
            let now = Instant::now();
            let index = st.browser.confirms.iter().position(|c| c.view.id == id && c.expires_at > now).ok_or(ErrorCode::BrowserPairExpired)?;
            let mut pending = st.browser.confirms.remove(index);
            if let Some(sender) = pending.outcome.take() {
                let _ = sender.send(true);
            }
        }
        self.changed();
        Ok(())
    }

    /// The user refuses a waiting fill or copy.
    pub fn browser_confirm_deny(&self, id: Uuid) -> CoreResult<()> {
        self.browser_command()?;
        {
            let mut st = self.lock();
            let index = st.browser.confirms.iter().position(|c| c.view.id == id).ok_or(ErrorCode::BrowserPairExpired)?;
            let mut pending = st.browser.confirms.remove(index);
            if let Some(sender) = pending.outcome.take() {
                let _ = sender.send(false);
            }
        }
        self.changed();
        Ok(())
    }

    /// The paused extensions get answers again.
    pub fn browser_resume(&self) -> CoreResult<()> {
        self.browser_command()?;
        {
            let mut st = self.lock();
            super::unlocked(&st)?;
            st.browser.paused.clear();
        }
        self.changed();
        Ok(())
    }

    /// Empty the activity list.
    pub fn browser_activity_clear(&self) -> CoreResult<()> {
        self.browser_command()?;
        {
            let mut st = self.lock();
            super::unlocked(&st)?;
            st.browser.activity.clear();
        }
        self.changed();
        Ok(())
    }

    /// Change what the extensions may do. A change that lets them do more (filling as pages load,
    /// submitting, filling codes, frames of other sites, no confirmation in the page) asks for the
    /// master password or the biometric check first.
    pub async fn browser_set_policy(&self, next: BrowserPolicy, password: Option<Zeroizing<String>>, reason: Option<String>) -> CoreResult<()> {
        self.browser_command()?;
        let riskier = {
            let st = self.lock();
            let vault = super::unlocked(&st)?;
            policy(vault).riskier(&next)
        };
        if riskier {
            self.confirm_presence(password, reason).await?;
        }
        {
            let mut guard = self.lock();
            let PhaseState::Unlocked(session) = &mut guard.phase else { return Err(ErrorCode::Locked.into()) };
            let before = std::mem::replace(&mut session.data.local_mut().browser.policy, next);
            if let Err(error) = self.write_vault(session) {
                session.data.local_mut().browser.policy = before;
                return Err(error);
            }
        }
        self.changed();
        Ok(())
    }

    /// Settings › Browser, as the state shows it.
    pub(super) fn browser_view(&self, st: &State) -> BrowserView {
        let available = self.browser_offered();
        let enabled = available && st.settings.browser_integration;
        let PhaseState::Unlocked(session) = &st.phase else { return BrowserView { available, enabled, ..BrowserView::default() } };
        if !available {
            return BrowserView::default();
        }
        let now = Instant::now();
        let local = session.data.local.as_ref().map(|l| &l.browser);
        let paired = local
            .map(|b| {
                b.paired
                    .iter()
                    .map(|p| PairedView {
                        id: p.id,
                        browser: p.browser.clone(),
                        label: p.label.clone(),
                        extension: p.extension.clone(),
                        paired_at_ms: p.paired_at_ms,
                        last_seen_ms: p.last_seen_ms,
                        connected: st.browser.connected(p.id),
                        page_access: st.browser.page_access.get(&p.id).copied(),
                        paused: st.browser.paused.contains(&p.id),
                    })
                    .collect()
            })
            .unwrap_or_default();
        BrowserView {
            available,
            enabled,
            paired,
            pairing: st.browser.pairing.iter().filter(|p| p.expires_at > now).map(PendingPair::view).collect(),
            confirms: st.browser.confirms.iter().filter(|c| c.expires_at > now).map(|c| c.view.clone()).collect(),
            policy: local.map(|b| b.policy.clone()).unwrap_or_default(),
            activity: st.browser.activity.iter().map(Activity::view).collect(),
        }
    }

    // ---- the steps of a fill ------------------------------------------------------------------

    /// Every check a fill goes through, in order; the login to fill and whether to ask first.
    fn decide_fill(&self, st: &mut State, session: SessionId, request: &FillRequest, frame: &Frame) -> Result<(Uuid, Decision), Refusal> {
        let now = Instant::now();
        let site = site_of(&frame.frame);
        let (vault, state) = active(st, session, false)?;
        let data = &vault.data;
        let policy = policy(vault);
        if listed(data, SiteRuleKind::NeverFill, &frame.frame) || listed(data, SiteRuleKind::NeverFill, &frame.top) {
            return Err(Refusal::NeverFill);
        }
        let cross = frame.cross_origin();
        if cross && policy.frames() == FrameRule::Never {
            return Err(Refusal::CrossOrigin);
        }
        let (_, found) = matches(data, &frame.frame);
        let (item_id, downgrade) = match request.item_id {
            Some(id) => found.iter().copied().find(|(found, _)| *found == id).ok_or(Refusal::NoMatch)?,
            None => found.first().copied().ok_or(Refusal::NoMatch)?,
        };
        let item = data.items.iter().find(|i| i.id == item_id).ok_or(Refusal::NotFound)?;
        if request.trigger == Trigger::PageLoad {
            page_load_allowed(&policy, item, frame, found.len(), downgrade)?;
        } else if downgrade && !request.allow_http {
            return Err(Refusal::HttpDowngrade);
        }
        let protected = item.protection_level() == Protection::Confirm;
        let mut reasons = Vec::new();
        if protected {
            reasons.push(ConfirmReason::Protected);
        }
        if cross {
            reasons.push(ConfirmReason::CrossOrigin);
        }
        let view = (!reasons.is_empty()).then(|| ConfirmView {
            id: Uuid::new_v4(),
            item_id,
            title: item.title.clone(),
            username: username_of(item),
            frame_url: frame.frame.as_str().to_owned(),
            top_url: cross.then(|| frame.top.as_str().to_owned()),
            browser: state.client.browser.clone(),
            label: state.client.label.clone(),
            reasons,
            action: ConfirmAction::Fill,
            needs_proof: protected,
            expires_at_ms: self.now_ms() + millis(CONFIRM_TTL),
        });
        if !st.browser.sessions.get_mut(&session.0).is_some_and(|s| s.secret(&site, now)) {
            return Err(self.pause(st, session, &site));
        }
        Ok((
            item_id,
            match view {
                Some(view) => self.ask(st, session, view),
                None => Decision::Now,
            },
        ))
    }

    /// Wait for the user's answer to a request that asks first.
    async fn wait(&self, decision: Decision) -> Result<(), Refusal> {
        let Decision::Ask { id, outcome, deadline } = decision else { return Ok(()) };
        let _waiting = Waiting { core: self, pair: None, confirm: Some(id) };
        self.changed();
        match timeout_at(deadline, outcome).await {
            Ok(Ok(true)) => Ok(()),
            Ok(Ok(false)) => Err(Refusal::Denied),
            Ok(Err(_)) => Err(if matches!(self.lock().phase, PhaseState::Unlocked(_)) { Refusal::Denied } else { Refusal::Locked }),
            Err(_) => Err(Refusal::Expired),
        }
    }

    /// The fill itself, once allowed: the secrets, the linked code (filled and copied as the policy
    /// and the login say), the copy time, the activity.
    fn complete_fill(&self, session: SessionId, item_id: Uuid, frame: &Frame, trigger: Trigger) -> Result<FillAnswer, Refusal> {
        let now = Instant::now();
        let now_ms = self.now_ms();
        let site = site_of(&frame.frame);
        let (answer, copied) = {
            let mut guard = self.lock();
            let st = &mut *guard;
            let (vault, _) = active(st, session, false)?;
            let policy = policy(vault);
            let item = vault.data.items.iter().find(|i| i.id == item_id && offered(i)).ok_or(Refusal::NotFound)?;
            let page_load = trigger == Trigger::PageLoad;
            let code = item.totp_entry.and_then(|e| vault.data.get(e)).map(|entry| current_code(entry, now_ms));
            let otp = code.clone().filter(|_| !page_load && choose(&item.autofill.fill_otp, policy.fill_otp));
            let answer = FillAnswer {
                item_id,
                username: username_of(item),
                password: item.field_of(Purpose::Password).map(|f| f.value.clone()).unwrap_or_default(),
                fields: item
                    .fields
                    .iter()
                    .filter(|f| !f.removed && !f.value.is_empty() && f.purpose == crate::Lenient::Known(Purpose::Custom))
                    .filter_map(|f| f.page_field.clone().map(|page_field| FillField { page_field, value: f.value.clone() }))
                    .collect(),
                otp,
                submit: !page_load && choose(&item.autofill.auto_submit, policy.auto_submit),
            };
            let title = item.title.clone();
            let copy = code.filter(|_| !page_load && policy.copy_otp);
            let copied = match &copy {
                Some(code) => Some(self.copy_secret(st, code)?),
                None => None,
            };
            if let PhaseState::Unlocked(vault) = &mut st.phase
                && let Some(item) = vault.data.items.iter_mut().find(|i| i.id == item_id)
            {
                item.last_used_at_ms = Some(now_ms);
                // Recency only: no backup or sync for a fill, and a failed write loses nothing.
                if let Err(error) = self.write_vault(vault) {
                    tracing::warn!(?error, "could not record the fill time");
                }
            }
            touch_items(st);
            if let Some(state) = st.browser.sessions.get_mut(&session.0) {
                state.filled(item_id, &site, now);
            }
            self.record(st, session, &site, ActivityEvent::Filled, Some(item_id), Some(title));
            (answer, copied)
        };
        self.changed();
        if let Some(clear_after) = copied {
            self.notice(Notice::ItemCopied { item_id: Some(item_id), part: CopiedPart::Totp, clear_after_s: clear_after });
        }
        Ok(answer)
    }

    // ---- helpers --------------------------------------------------------------------------------

    /// Put a request in front of the user.
    fn ask(&self, st: &mut State, session: SessionId, view: ConfirmView) -> Decision {
        let (sender, outcome) = oneshot::channel();
        let deadline = Instant::now() + CONFIRM_TTL;
        let id = view.id;
        let notice = Notice::BrowserConfirmRequested { id, browser: view.browser.clone(), title: view.title.clone() };
        st.browser.confirms.push(PendingConfirm { view, session: session.0, expires_at: deadline, outcome: Some(sender) });
        self.notice(notice);
        Decision::Ask { id, outcome, deadline }
    }

    /// Write `text` to the clipboard, cleared after the clipboard time as a code is.
    fn copy_secret(&self, st: &mut State, text: &str) -> Result<Option<u32>, Refusal> {
        self.shared.ports.clipboard.set_secret_text(text).map_err(|_| Refusal::Unavailable)?;
        let clear_seconds = st.settings.clipboard_clear_seconds;
        st.clipboard = (clear_seconds > 0).then(|| (Zeroizing::new(text.to_owned()), Instant::now() + Duration::from_secs(u64::from(clear_seconds))));
        Ok((clear_seconds > 0).then_some(clear_seconds))
    }

    /// The session asked too often: its pairing is paused until the user resumes it.
    fn pause(&self, st: &mut State, session: SessionId, site: &str) -> Refusal {
        let Some(state) = st.browser.sessions.get(&session.0) else { return Refusal::NotPaired };
        let (pairing, browser, label) = (state.pairing, state.client.browser.clone(), state.client.label.clone());
        if st.browser.paused.insert(pairing) {
            self.record(st, session, site, ActivityEvent::Paused, None, None);
            self.notice(Notice::BrowserPaused { pairing, browser, label });
        }
        Refusal::RateLimited
    }

    fn refused(&self, st: &mut State, session: SessionId, site: &str, refusal: Refusal, item_id: Option<Uuid>) {
        if matches!(refusal, Refusal::Locked | Refusal::NotPaired | Refusal::Unavailable | Refusal::RateLimited | Refusal::Paused) {
            return;
        }
        let title = item_id.and_then(|id| item_title(st, id));
        let at_ms = self.now_ms();
        let Some(state) = st.browser.sessions.get(&session.0) else { return };
        let entry = Activity {
            at_ms,
            browser: state.client.browser.clone(),
            label: state.client.label.clone(),
            site: site.to_owned(),
            event: ActivityEvent::Refused,
            refusal: Some(refusal),
            item_id,
            title,
        };
        st.browser.record(entry);
    }

    fn record(&self, st: &mut State, session: SessionId, site: &str, event: ActivityEvent, item_id: Option<Uuid>, title: Option<String>) {
        let at_ms = self.now_ms();
        let Some(state) = st.browser.sessions.get(&session.0) else { return };
        let entry = Activity {
            at_ms,
            browser: state.client.browser.clone(),
            label: state.client.label.clone(),
            site: site.to_owned(),
            event,
            refusal: None,
            item_id,
            title,
        };
        st.browser.record(entry);
    }

    fn saved(&self, session: SessionId, site: &str, item_id: Uuid, title: String, updated: bool) {
        {
            let mut st = self.lock();
            let event = if updated { ActivityEvent::Updated } else { ActivityEvent::Saved };
            self.record(&mut st, session, site, event, Some(item_id), Some(title.clone()));
        }
        self.changed();
        self.notice(Notice::ItemSaved { item_id, title, updated });
    }
}

/// The session's vault and state, when it may be answered: unlocked, the session open, its
/// pairing still there, and (unless `paused_too`) not paused.
fn active(st: &State, session: SessionId, paused_too: bool) -> Result<(&Session, &SessionState), Refusal> {
    let PhaseState::Unlocked(vault) = &st.phase else { return Err(Refusal::Locked) };
    let state = st.browser.sessions.get(&session.0).ok_or(Refusal::NotPaired)?;
    paired(vault, state.pairing).ok_or(Refusal::NotPaired)?;
    if !paused_too && st.browser.paused.contains(&state.pairing) {
        return Err(Refusal::Paused);
    }
    Ok((vault, state))
}

fn paired(session: &Session, pairing: Uuid) -> Option<&PairedExtension> {
    session.data.local.as_ref()?.browser.paired.iter().find(|p| p.id == pairing)
}

fn policy(session: &Session) -> BrowserPolicy {
    session.data.local.as_ref().map(|l| l.browser.policy.clone()).unwrap_or_default()
}

/// A fill as the page loads goes only where everything is plain: the policy and the login allow
/// it, the form is in the page itself, exactly one login matches, it fills without asking, and
/// the page is `https` (or `http` like the URL saved).
fn page_load_allowed(policy: &BrowserPolicy, item: &VaultItem, frame: &Frame, matched: usize, downgrade: bool) -> Result<(), Refusal> {
    let own = item.autofill.on_page_load.or(Tri::Default);
    let saved_plainly =
        frame.frame.is_https() || item.urls.iter().any(|u| WebUrl::parse_saved(&u.url).is_some_and(|saved| !saved.is_https()) && u.rule() != Matching::Regex);
    let allowed = policy.on_page_load
        && own != Tri::Off
        && frame.top_level()
        && matched == 1
        && item.protection_level() == Protection::Fill
        && !downgrade
        && saved_plainly;
    if allowed { Ok(()) } else { Err(Refusal::Protected) }
}

/// What a submitted form calls for: nothing when the login is saved already (or the site is on
/// the never-save list), an update for a login of the same user name, otherwise a new login.
fn advice(data: &crate::entry::VaultData, found: &[(Uuid, bool)], never_save: bool, username: &str, password: &str) -> SaveAdvice {
    let none = SaveAdvice { action: SaveAction::None, item_id: None };
    if never_save || password.is_empty() {
        return none;
    }
    let username = username.trim();
    let items = found.iter().filter_map(|(id, _)| data.items.iter().find(|i| i.id == *id)).collect::<Vec<_>>();
    let saved_password = |item: &VaultItem| item.field_of(Purpose::Password).is_some_and(|f| f.value.as_str() == password);
    if username.is_empty() {
        if items.iter().any(|i| saved_password(i)) {
            return none;
        }
        return SaveAdvice { action: SaveAction::Save, item_id: None };
    }
    let same_user = items.iter().find(|i| {
        let name = |purpose| i.field_of(purpose).map(|f| f.value.to_lowercase()).unwrap_or_default();
        name(Purpose::Username) == username.to_lowercase() || name(Purpose::Email) == username.to_lowercase()
    });
    match same_user {
        Some(item) if saved_password(item) => none,
        Some(item) => SaveAdvice { action: SaveAction::Update, item_id: Some(item.id) },
        None => SaveAdvice { action: SaveAction::Save, item_id: None },
    }
}

fn summary(item: &VaultItem, downgrade: bool, matches: bool) -> ItemSummary {
    ItemSummary {
        item_id: item.id,
        title: item.title.clone(),
        username: item.field_of(Purpose::Username).map(|f| f.value.to_string()).unwrap_or_default(),
        email: item.field_of(Purpose::Email).map(|f| f.value.to_string()).unwrap_or_default(),
        has_password: item.field_of(Purpose::Password).is_some_and(|f| !f.value.is_empty()),
        has_totp: item.totp_entry.is_some(),
        protection: item.protection_level(),
        favorite: item.favorite,
        downgrade,
        matches,
    }
}

/// The name a form gets: the user name, or the email address when there is none.
fn username_of(item: &VaultItem) -> String {
    item.field_of(Purpose::Username).filter(|f| !f.value.is_empty()).or_else(|| item.field_of(Purpose::Email)).map(|f| f.value.to_string()).unwrap_or_default()
}

fn searched(item: &VaultItem, text: &str) -> bool {
    let name = |purpose| item.field_of(purpose).is_some_and(|f| f.value.to_lowercase().contains(text));
    item.title.to_lowercase().contains(text)
        || name(Purpose::Username)
        || name(Purpose::Email)
        || item.urls.iter().any(|u| u.url.to_lowercase().contains(text))
        || item.tags.iter().any(|t| t.to_lowercase().contains(text))
}

fn item_title(st: &State, id: Uuid) -> Option<String> {
    let PhaseState::Unlocked(vault) = &st.phase else { return None };
    vault.data.items.iter().find(|i| i.id == id).map(|i| i.title.clone())
}

/// The scheme and host of a page, as a new login keeps it.
fn origin(page: &WebUrl) -> String {
    let scheme = if page.is_https() { "https" } else { "http" };
    match page.port() {
        Some(port) => format!("{scheme}://{}:{port}/", page.host()),
        None => format!("{scheme}://{}/", page.host()),
    }
}

/// 32 random bytes from the system's generator.
fn random_key() -> Option<Zeroizing<[u8; 32]>> {
    let mut random = OsRandom::new();
    let mut key = Zeroizing::new([0u8; 32]);
    for chunk in key.chunks_mut(4) {
        chunk.copy_from_slice(&random.next_u32().ok()?.to_le_bytes());
    }
    Some(key)
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
