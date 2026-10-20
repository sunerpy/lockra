//! The browser extension's side of the core: the static key, pairing, the decisions on fills and
//! the other requests of a session, the rate limits, the policy, and what the interface sees. The
//! channel itself (Noise, framing) is lockra-browser's and has its own tests.

use std::time::Duration;

use super::*;
use crate::browser::{
    ActivityEvent, BrowserPolicy, ClientInfo, ConfirmAction, ConfirmReason, CopyGeneratedRequest, CopyRequest, DeleteRequest, FILLS_PER_MINUTE, FillRequest,
    FrameFacts, FrameRule, GenerateRequest, GenerateSave, LinkCodeRequest, OtpRequest, QUERIES_PER_MINUTE, QueryPurpose, QueryRequest, Refusal,
    SITES_PER_TEN_MINUTES, SaveAction, SaveAdvice, SaveRequest, SearchRequest, SessionId, SiteMatchingRequest, SiteRuleRequest, Trigger, UpdateRequest,
};
use crate::item::{Matching, Protection, SiteRule, SiteRuleKind, Tri};
use crate::logins::{CopyPart, ItemDraft, ItemPatch, RevealParts, SecretChange, UrlDraft};
use crate::{AutofillChoice, Lenient};

const KEY: [u8; 32] = [7; 32];
const PASSWORD: &str = "hunter2-not-for-the-page";

fn chrome() -> ClientInfo {
    ClientInfo { browser: "Chrome".into(), label: "Work".into(), extension: "abcdefghijklmnop".into(), version: "0.10.0".into(), page_access: true }
}

fn facts(frame: &str) -> FrameFacts {
    FrameFacts { frame_url: frame.into(), top_url: frame.into(), frame_chain: Vec::new() }
}

fn fill_at(url: &str) -> FillRequest {
    FillRequest { facts: facts(url), ..FillRequest::default() }
}

fn login(title: &str, username: &str, url: &str, matching: Matching) -> ItemDraft {
    ItemDraft {
        title: title.into(),
        username: username.into(),
        password: SecretChange::Set { value: pw(PASSWORD) },
        urls: vec![UrlDraft { url: url.into(), matching }],
        ..ItemDraft::default()
    }
}

/// A paired extension with an open session.
async fn paired(h: &Harness) -> (Uuid, SessionId) {
    let core = h.core.clone();
    let pairing = tokio::spawn(async move { core.browser_pair(chrome(), KEY, "123456".into()).await });
    settle().await;
    let request = h.core.state().browser.pairing[0].id;
    h.core.browser_pair_approve(request, Some(pw(MASTER)), None).await.unwrap();
    let pairing = pairing.await.unwrap().unwrap();
    let session = h.core.browser_session_open(pairing, chrome()).unwrap();
    (pairing, session)
}

async fn riskier(h: &Harness, change: impl FnOnce(&mut BrowserPolicy)) {
    let mut policy = h.core.state().browser.policy;
    change(&mut policy);
    h.core.browser_set_policy(policy, Some(pw(MASTER)), None).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn the_static_key_is_made_once_kept_in_the_vault_and_given_only_while_unlocked() {
    let h = Harness::unlocked().await;
    let key = h.core.browser_static_secret().unwrap();
    assert_eq!(h.core.browser_static_secret().unwrap(), key, "made once");
    assert_ne!(*key, [0; 32]);
    // Nowhere the webview or a backup could see it.
    let file = h.dir.path().join("keyless.lockrabackup");
    h.core.backup_to(file.clone(), None).await.unwrap();
    let state = serde_json::to_string(&h.core.state()).unwrap();
    assert!(!state.contains(&data_encoding::BASE64.encode(key.as_slice())));
    let backup = crate::entry::VaultData::from_bytes(&h.core.data().backup_bytes()).unwrap();
    assert!(backup.local.is_none(), "a backup carries no local part");
    h.core.lock_vault();
    assert!(h.core.browser_static_secret().is_none(), "locked: no key");
    let restarted = h.restart();
    restarted.unlock(pw(MASTER)).await.unwrap();
    assert_eq!(restarted.browser_static_secret().unwrap(), key, "kept in the vault");
}

#[tokio::test(start_paused = true)]
async fn a_pairing_waits_for_the_user_who_approves_it_with_proof() {
    let mut h = Harness::unlocked().await;
    let _ = h.notices();
    let core = h.core.clone();
    let pairing = tokio::spawn(async move { core.browser_pair(chrome(), KEY, "654321".into()).await });
    settle().await;
    let state = h.core.state();
    let request = state.browser.pairing[0].clone();
    assert_eq!((request.code.as_str(), request.browser.as_str(), request.label.as_str()), ("654321", "chrome", "Work"));
    assert_eq!(request.expires_at_ms, T0 + 120_000);
    assert!(h.notices().contains(&Notice::BrowserPairRequested { id: request.id, browser: "chrome".into(), code: "654321".into() }));
    // Approving needs proof: no biometric check here, so the master password.
    assert_eq!(code_err(h.core.browser_pair_approve(request.id, None, None).await), ErrorCode::BiometricUnavailable);
    assert_eq!(code_err(h.core.browser_pair_approve(request.id, Some(pw("wrong password")), None).await), ErrorCode::WrongPassword);
    assert!(!pairing.is_finished(), "still waiting");
    h.core.browser_pair_approve(request.id, Some(pw(MASTER)), None).await.unwrap();
    let id = pairing.await.unwrap().unwrap();
    let state = h.core.state();
    assert!(state.browser.pairing.is_empty());
    let paired = &state.browser.paired[0];
    assert_eq!((paired.id, paired.browser.as_str(), paired.connected, paired.paired_at_ms), (id, "chrome", false, T0));
    assert_eq!(h.core.browser_peer(id), Some(KEY));
    assert_eq!(state.browser.activity[0].event, ActivityEvent::Paired);
    // Pairing the same extension again replaces its pairing.
    let core = h.core.clone();
    let again = tokio::spawn(async move { core.browser_pair(chrome(), KEY, "111111".into()).await });
    settle().await;
    let request = h.core.state().browser.pairing[0].id;
    h.core.browser_pair_approve(request, Some(pw(MASTER)), None).await.unwrap();
    let second = again.await.unwrap().unwrap();
    assert_eq!(h.core.state().browser.paired.iter().map(|p| p.id).collect::<Vec<_>>(), [second]);
    assert_eq!(h.core.browser_peer(id), None);
    // Kept in the vault.
    let restarted = h.restart();
    restarted.unlock(pw(MASTER)).await.unwrap();
    assert_eq!(restarted.browser_peer(second), Some(KEY));
}

#[tokio::test(start_paused = true)]
async fn a_pairing_request_is_denied_or_lapses_and_at_most_three_wait() {
    let h = Harness::unlocked().await;
    let mut waiting = Vec::new();
    for n in 0..3 {
        let core = h.core.clone();
        waiting.push(tokio::spawn(async move { core.browser_pair(chrome(), [n; 32], format!("00000{n}")).await }));
    }
    settle().await;
    assert_eq!(h.core.browser_pair(chrome(), KEY, "999999".into()).await, Err(Refusal::Busy));
    let first = h.core.state().browser.pairing[0].id;
    h.core.browser_pair_deny(first).unwrap();
    assert_eq!(waiting.remove(0).await.unwrap(), Err(Refusal::Denied));
    advance(Duration::from_secs(121)).await;
    for request in waiting {
        assert_eq!(request.await.unwrap(), Err(Refusal::Expired));
    }
    assert!(h.core.state().browser.pairing.is_empty(), "gone from the interface");
    assert_eq!(code_err(h.core.browser_pair_approve(first, Some(pw(MASTER)), None).await), ErrorCode::BrowserPairExpired);
    assert_eq!(code_err(h.core.browser_pair_deny(first)), ErrorCode::BrowserPairExpired);
    // The channel gave up (the extension went away): the request goes with it.
    let core = h.core.clone();
    let gone = tokio::spawn(async move { core.browser_pair(chrome(), KEY, "222222".into()).await });
    settle().await;
    assert_eq!(h.core.state().browser.pairing.len(), 1);
    gone.abort();
    settle().await;
    assert!(h.core.state().browser.pairing.is_empty());
}

#[tokio::test(start_paused = true)]
async fn nothing_is_paired_or_answered_while_locked_and_locking_ends_it_all() {
    let h = Harness::unlocked().await;
    let (pairing, session) = paired(&h).await;
    h.core.item_add(login("GitHub", "octocat", "https://github.com/", Matching::Domain)).unwrap();
    h.core.browser_fill(session, fill_at("https://github.com/login")).await.unwrap();
    let core = h.core.clone();
    let request = tokio::spawn(async move { core.browser_pair(chrome(), [9; 32], "333333".into()).await });
    settle().await;
    assert!(h.core.browser_session_alive(session));
    assert!(!h.core.state().browser.activity.is_empty());
    h.core.lock_vault();
    assert_eq!(request.await.unwrap(), Err(Refusal::Locked));
    assert!(!h.core.browser_session_alive(session));
    assert_eq!(h.core.browser_fill(session, fill_at("https://github.com/login")).await, Err(Refusal::Locked));
    assert_eq!(h.core.browser_peer(pairing), None);
    assert_eq!(h.core.browser_session_open(pairing, chrome()), Err(Refusal::Locked));
    assert_eq!(h.core.browser_pair(chrome(), KEY, "444444".into()).await, Err(Refusal::Locked));
    let state = h.core.state();
    assert!(state.browser.paired.is_empty() && state.browser.activity.is_empty(), "nothing shown while locked");
    h.core.unlock(pw(MASTER)).await.unwrap();
    let state = h.core.state();
    assert!(state.browser.activity.is_empty(), "the activity list is emptied as the vault locks");
    assert!(!state.browser.paired[0].connected, "the session ended");
    assert_eq!(h.core.browser_session_open(pairing, chrome()).map(|_| ()), Ok(()), "a new session opens");
}

#[tokio::test(start_paused = true)]
async fn a_fill_gets_the_best_match_for_the_frame_and_nothing_else() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let site = h.core.item_add(login("GitHub", "octocat", "https://github.com/", Matching::Domain)).unwrap();
    let gist = h.core.item_add(login("Gist", "gister", "https://gist.github.com/", Matching::Host)).unwrap();
    let hidden =
        h.core.item_add(ItemDraft { protection: Protection::NoExtension, ..login("Hidden", "h", "https://gist.github.com/", Matching::Exact) }).unwrap();
    // On github.com only the domain rule matches; on gist the host rule is closer and wins alone.
    assert_eq!(h.core.browser_fill(session, fill_at("https://github.com/login")).await.unwrap().item_id, site);
    let answer = h.core.browser_fill(session, fill_at("https://gist.github.com/")).await.unwrap();
    assert_eq!((answer.item_id, answer.username.as_str(), answer.password.as_str()), (gist, "gister", PASSWORD));
    let named = FillRequest { item_id: Some(site), ..fill_at("https://gist.github.com/") };
    assert_eq!(h.core.browser_fill(session, named).await, Err(Refusal::NoMatch), "a looser match is not offered");
    let not_offered = FillRequest { item_id: Some(hidden), ..fill_at("https://gist.github.com/") };
    assert_eq!(h.core.browser_fill(session, not_offered).await, Err(Refusal::NoMatch), "no_extension is never handed over");
    assert_eq!(h.core.browser_fill(session, fill_at("https://example.org/")).await, Err(Refusal::NoMatch));
    for page in ["about:blank", "data:text/html,x", "chrome-extension://abc/x.html", "not a url"] {
        assert_eq!(h.core.browser_fill(session, fill_at(page)).await, Err(Refusal::Invalid), "{page}");
    }
    // The query shows the same, without a secret.
    let query = h.core.browser_query(session, QueryRequest { facts: facts("https://gist.github.com/"), ..QueryRequest::default() }).unwrap();
    assert_eq!(query.items.iter().map(|i| i.item_id).collect::<Vec<_>>(), [gist]);
    assert_eq!((query.site.as_str(), query.level), ("github.com", Some(lockra_autofill::MatchLevel::Host)));
    assert!(!serde_json::to_string(&query).unwrap().contains(PASSWORD));
    // The never-fill list stops it before matching.
    h.core.site_rule_add(SiteRuleKind::NeverFill, &["github.com".into()]).unwrap();
    assert_eq!(h.core.browser_fill(session, fill_at("https://gist.github.com/")).await, Err(Refusal::NeverFill));
    let query = h.core.browser_query(session, QueryRequest { facts: facts("https://gist.github.com/"), ..QueryRequest::default() }).unwrap();
    assert!(query.never_fill && query.items.is_empty());
    let refused = &h.core.state().browser.activity[0];
    assert_eq!((refused.event, refused.refusal, refused.site.as_str()), (ActivityEvent::Refused, Some(Refusal::NeverFill), "github.com"));
}

#[tokio::test(start_paused = true)]
async fn with_the_port_switch_each_port_of_an_address_has_its_own_logins() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let nas = h.core.item_add(login("NAS", "admin", "http://192.168.1.10:5000/", Matching::Domain)).unwrap();
    let router = h.core.item_add(login("Router", "root", "http://192.168.1.10/", Matching::Domain)).unwrap();
    let query = |page: &str| {
        let answer = h.core.browser_query(session, QueryRequest { facts: facts(page), ..QueryRequest::default() }).unwrap();
        answer.items.iter().map(|i| i.item_id).collect::<Vec<_>>()
    };
    // Off (the default): one address, so both logins on each of its ports.
    assert_eq!(query("http://192.168.1.10:5000/").len(), 2);
    // On, without proof (it only narrows what the extensions get): each port its own logins.
    let on = BrowserPolicy { match_port: true, ..h.core.state().browser.policy };
    h.core.browser_set_policy(on.clone(), None, None).await.unwrap();
    assert_eq!(query("http://192.168.1.10:5000/login"), [nas]);
    assert_eq!(query("http://192.168.1.10/"), [router]);
    assert!(query("http://192.168.1.10:8080/").is_empty());
    let named = FillRequest { item_id: Some(router), ..fill_at("http://192.168.1.10:5000/") };
    assert_eq!(h.core.browser_fill(session, named).await, Err(Refusal::NoMatch));
    assert_eq!(h.core.browser_fill(session, fill_at("http://192.168.1.10:5000/")).await.unwrap().item_id, nas);
    // Off again is the default, which needs no proof either.
    h.core.browser_set_policy(BrowserPolicy { match_port: false, ..on }, None, None).await.unwrap();
    assert_eq!(query("http://192.168.1.10:5000/").len(), 2);
}

/// The logins a query offers on `page`, its level, and whether the frame matches by its host.
fn offered_on(h: &Harness, session: SessionId, page: &str) -> (Vec<Uuid>, Option<lockra_autofill::MatchLevel>, bool) {
    let answer = h.core.browser_query(session, QueryRequest { facts: facts(page), ..QueryRequest::default() }).unwrap();
    (answer.items.iter().map(|i| i.item_id).collect(), answer.level, answer.host_matching)
}

/// The site rules, as the interface lists them.
fn site_rules(h: &Harness) -> Vec<(String, Vec<String>)> {
    h.core.state().site_rules.into_iter().map(|r| (r.kind, r.domains)).collect()
}

fn rule_of(kind: &str, domain: &str) -> (String, Vec<String>) {
    (kind.to_owned(), vec![domain.to_owned()])
}

#[tokio::test(start_paused = true)]
async fn by_the_full_host_each_host_of_a_site_has_its_own_logins() {
    // 2026-10-20: a company's domain offered every one of its eleven logins on each of its hosts.
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let entry = h.core.add_uri(&otpauth("GitLab", "dev", SECRET)).unwrap();
    let git = h.core.item_add(ItemDraft { totp_entry: Some(entry), ..login("GitLab", "dev", "https://git.example.cn/", Matching::Domain) }).unwrap();
    let wiki = h.core.item_add(login("Wiki", "writer", "https://wiki.example.cn/", Matching::Domain)).unwrap();
    let page = "https://git.example.cn/users/sign_in";
    // By the site, the default: both logins on each host.
    let (found, level, host) = offered_on(&h, session, page);
    assert_eq!((found.len(), level, host), (2, Some(lockra_autofill::MatchLevel::Domain), false));
    // By the full host, the policy's default now (without proof: it only narrows the matches).
    let on = BrowserPolicy { match_host: true, ..h.core.state().browser.policy };
    h.core.browser_set_policy(on.clone(), None, None).await.unwrap();
    assert_eq!(offered_on(&h, session, page), (vec![git], Some(lockra_autofill::MatchLevel::Host), true));
    assert_eq!(offered_on(&h, session, "https://wiki.example.cn/").0, [wiki]);
    assert_eq!(offered_on(&h, session, "https://example.cn/"), (Vec::new(), None, true));
    // The other requests go by the same matches: the fill, the code, the update, the deletion and
    // the popup's search.
    let named = FillRequest { item_id: Some(wiki), ..fill_at(page) };
    assert_eq!(h.core.browser_fill(session, named).await, Err(Refusal::NoMatch));
    assert_eq!(h.core.browser_fill(session, fill_at(page)).await.unwrap().item_id, git);
    let code = OtpRequest { item_id: git, facts: facts("https://wiki.example.cn/"), trigger: Some(Trigger::Popup) };
    assert_eq!(h.core.browser_otp(session, code), Err(Refusal::NoMatch));
    let update = UpdateRequest { item_id: wiki, facts: facts(page), password: pw("n3w") };
    assert_eq!(h.core.browser_update(session, update), Err(Refusal::NoMatch));
    assert_eq!(h.core.browser_delete(session, DeleteRequest { item_id: wiki, facts: facts(page) }).await, Err(Refusal::NoMatch));
    let found = h.core.browser_search(session, SearchRequest { text: "example.cn".into(), top_url: page.into() }).unwrap();
    assert_eq!(found.iter().map(|i| (i.item_id, i.matches)).collect::<Vec<_>>(), [(git, true), (wiki, false)]);
    // Kept in the vault, as the rest of the policy.
    let restarted = h.restart();
    restarted.unlock(pw(MASTER)).await.unwrap();
    assert!(restarted.state().browser.policy.match_host);
}

#[tokio::test(start_paused = true)]
async fn a_site_rule_chooses_how_its_domain_matches_whatever_the_default() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let git = h.core.item_add(login("GitLab", "dev", "https://git.example.cn/", Matching::Domain)).unwrap();
    let wiki = h.core.item_add(login("Wiki", "writer", "https://wiki.example.cn/", Matching::Domain)).unwrap();
    let shop = h.core.item_add(login("Shop", "me", "https://shop.example/", Matching::Domain)).unwrap();
    let www = h.core.item_add(login("Shop (www)", "me", "https://www.shop.example/", Matching::Domain)).unwrap();
    // The company's domain by the full host, every other site by the site still.
    h.core.site_rule_add(SiteRuleKind::MatchHost, &["example.cn".into()]).unwrap();
    assert_eq!(site_rules(&h), [rule_of("match_host", "example.cn")]);
    assert_eq!(offered_on(&h, session, "https://git.example.cn/").0, [git]);
    assert_eq!(offered_on(&h, session, "https://wiki.example.cn/").0, [wiki]);
    assert_eq!(offered_on(&h, session, "https://www.shop.example/"), (vec![shop, www], Some(lockra_autofill::MatchLevel::Domain), false));
    // Every site by the full host, and the shop's domain by the site again.
    h.core.browser_set_policy(BrowserPolicy { match_host: true, ..h.core.state().browser.policy }, None, None).await.unwrap();
    assert_eq!(offered_on(&h, session, "https://www.shop.example/"), (vec![www], Some(lockra_autofill::MatchLevel::Host), true));
    h.core.site_rule_add(SiteRuleKind::MatchDomain, &["https://Shop.example/login".into()]).unwrap();
    assert_eq!(offered_on(&h, session, "https://www.shop.example/"), (vec![shop, www], Some(lockra_autofill::MatchLevel::Domain), false));
    assert!(!offered_on(&h, session, "https://shop.example/").2);
    // The rule of the longest domain wins: a company by the full host, its Git server by the domain.
    let sso = h.core.item_add(login("SSO", "me", "https://sso.company.com/", Matching::Domain)).unwrap();
    let docs = h.core.item_add(login("Docs", "me", "https://docs.company.com/", Matching::Domain)).unwrap();
    h.core.browser_set_policy(BrowserPolicy { match_host: false, ..h.core.state().browser.policy }, None, None).await.unwrap();
    h.core.site_rule_add(SiteRuleKind::MatchHost, &["company.com".into()]).unwrap();
    h.core.site_rule_add(SiteRuleKind::MatchDomain, &["git.company.com".into()]).unwrap();
    let (found, level, host) = offered_on(&h, session, "https://git.company.com/login");
    assert_eq!((found.len(), level, host), (2, Some(lockra_autofill::MatchLevel::Domain), false));
    assert!(found.contains(&sso) && found.contains(&docs));
    assert_eq!(offered_on(&h, session, "https://docs.company.com/"), (vec![docs], Some(lockra_autofill::MatchLevel::Host), true));
}

#[tokio::test(start_paused = true)]
async fn a_domain_matches_one_way_and_adding_the_other_removes_the_first() {
    let h = Harness::unlocked().await;
    h.core.site_rule_add(SiteRuleKind::MatchHost, &["Company.com.".into()]).unwrap();
    assert_eq!(site_rules(&h), [rule_of("match_host", "company.com")]);
    // The other way: the first rule goes in the same change, its deletion kept to sync.
    h.core.site_rule_add(SiteRuleKind::MatchDomain, &["company.com".into()]).unwrap();
    assert_eq!(site_rules(&h), [rule_of("match_domain", "company.com")]);
    let removed = SiteRule::single_id(SiteRuleKind::MatchHost, "company.com");
    assert!(h.core.data().site_rule_tombstones.iter().any(|t| t.id == removed));
    h.core.site_rule_add(SiteRuleKind::MatchHost, &["company.com".into()]).unwrap();
    assert_eq!(site_rules(&h), [rule_of("match_host", "company.com")]);
    // Another domain, or another kind about the same one, stays.
    h.core.site_rule_add(SiteRuleKind::NeverSave, &["company.com".into()]).unwrap();
    h.core.site_rule_add(SiteRuleKind::MatchDomain, &["git.company.com".into()]).unwrap();
    assert_eq!(site_rules(&h), [rule_of("match_host", "company.com"), rule_of("never_save", "company.com"), rule_of("match_domain", "git.company.com")]);
    // One domain a rule.
    assert_eq!(code_err(h.core.site_rule_add(SiteRuleKind::MatchHost, &["a.example".into(), "b.example".into()])), ErrorCode::ItemInvalid);
    assert_eq!(code_err(h.core.site_rule_add(SiteRuleKind::MatchDomain, &[])), ErrorCode::ItemInvalid);
    assert_eq!(site_rules(&h).len(), 3);
    // Kept, and the ids are the shared ones, as for the lists.
    let restarted = h.restart();
    restarted.unlock(pw(MASTER)).await.unwrap();
    assert!(restarted.state().site_rules.iter().any(|r| r.id == SiteRule::single_id(SiteRuleKind::MatchDomain, "git.company.com")));
}

#[tokio::test(start_paused = true)]
async fn the_popup_switches_how_the_frame_s_site_matches_and_leaves_no_rule_that_says_the_default() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let git = h.core.item_add(login("GitLab", "dev", "https://git.example.cn/", Matching::Domain)).unwrap();
    h.core.item_add(login("Wiki", "writer", "https://wiki.example.cn/", Matching::Domain)).unwrap();
    let page = "https://git.example.cn/users/sign_in";
    let switch = |host| h.core.browser_site_matching(session, SiteMatchingRequest { facts: facts(page), host });
    // By the full host on this site: one rule, for the site, which its other hosts follow too.
    switch(true).unwrap();
    assert_eq!(site_rules(&h), [rule_of("match_host", "example.cn")]);
    assert_eq!(offered_on(&h, session, page), (vec![git], Some(lockra_autofill::MatchLevel::Host), true));
    assert!(offered_on(&h, session, "https://wiki.example.cn/").2);
    switch(true).unwrap();
    assert_eq!(site_rules(&h), [rule_of("match_host", "example.cn")], "asked again: as it was");
    // Back to the site, which is the default: no rule left, its removal kept to sync.
    switch(false).unwrap();
    assert!(site_rules(&h).is_empty());
    assert_eq!(offered_on(&h, session, page).0.len(), 2);
    assert!(h.core.data().site_rule_tombstones.iter().any(|t| t.id == SiteRule::single_id(SiteRuleKind::MatchHost, "example.cn")));
    // With the default by the full host, the switch's rule says the other way.
    h.core.browser_set_policy(BrowserPolicy { match_host: true, ..h.core.state().browser.policy }, None, None).await.unwrap();
    assert!(offered_on(&h, session, page).2);
    switch(false).unwrap();
    assert_eq!(site_rules(&h), [rule_of("match_domain", "example.cn")]);
    assert!(!offered_on(&h, session, page).2);
    switch(true).unwrap();
    assert!(site_rules(&h).is_empty());
    // A rule about the frame's own host goes too, so the switch says what this page does; a rule
    // about another host of the site stays.
    h.core.site_rule_add(SiteRuleKind::MatchDomain, &["git.example.cn".into()]).unwrap();
    h.core.site_rule_add(SiteRuleKind::MatchDomain, &["wiki.example.cn".into()]).unwrap();
    assert!(!offered_on(&h, session, page).2);
    switch(true).unwrap();
    assert!(offered_on(&h, session, page).2);
    assert_eq!(site_rules(&h), [rule_of("match_domain", "wiki.example.cn")]);
    // Only in the page itself or a frame of its own site, and for a page at all.
    let framed = FrameFacts { frame_url: page.into(), top_url: "https://evil.example/".into(), frame_chain: Vec::new() };
    assert_eq!(h.core.browser_site_matching(session, SiteMatchingRequest { facts: framed, host: false }), Err(Refusal::CrossOrigin));
    let nested = FrameFacts { frame_url: page.into(), top_url: page.into(), frame_chain: vec!["https://ads.example/".into()] };
    assert_eq!(h.core.browser_site_matching(session, SiteMatchingRequest { facts: nested, host: false }), Err(Refusal::CrossOrigin));
    assert_eq!(h.core.browser_site_matching(session, SiteMatchingRequest { facts: facts("about:blank"), host: false }), Err(Refusal::Invalid));
    assert_eq!(site_rules(&h), [rule_of("match_domain", "wiki.example.cn")], "nothing changed");
}

#[tokio::test(start_paused = true)]
async fn the_switch_counts_as_a_query_and_waits_while_paused_or_locked() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let switch = SiteMatchingRequest { facts: facts("https://git.example.cn/"), host: true };
    for _ in 0..QUERIES_PER_MINUTE {
        h.core.browser_query(session, QueryRequest { facts: facts("https://git.example.cn/"), ..QueryRequest::default() }).unwrap();
    }
    assert_eq!(h.core.browser_site_matching(session, switch.clone()), Err(Refusal::RateLimited));
    assert_eq!(h.core.browser_site_matching(session, switch.clone()), Err(Refusal::Paused));
    assert!(site_rules(&h).is_empty());
    h.core.browser_resume().unwrap();
    h.core.lock_vault();
    assert_eq!(h.core.browser_site_matching(session, switch), Err(Refusal::Locked));
}

#[tokio::test(start_paused = true)]
async fn a_code_typed_into_a_password_field_is_never_offered_as_the_password() {
    // 2026-10-20: a sign-in that asks for the code in a password field offered to keep the code as
    // the login's new password.
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let entry = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    let id = h.core.item_add(ItemDraft { totp_entry: Some(entry), ..login("GitHub", "octocat", "https://github.com/", Matching::Domain) }).unwrap();
    let sink = Arc::new(RecordingSink::default());
    h.core.subscribe_codes(sink.clone());
    let frame = sink.frames().last().cloned().unwrap();
    let shown = frame.codes.iter().find(|c| c.entry_id == entry).unwrap().clone();
    let secret = lockra_otp::base32::decode(SECRET).unwrap();
    let at = |ms: u64| totp(&secret, ms, Period::THIRTY, Algorithm::Sha1, Digits::SIX);
    let ask = |username: &str, password: &str| QueryRequest {
        facts: facts("https://github.com/sessions/two-factor"),
        purpose: QueryPurpose::Save,
        username: username.into(),
        password: pw(password),
    };
    let advice = |q: QueryRequest| h.core.browser_query(session, q).unwrap().save.unwrap();
    let nothing = SaveAdvice { action: SaveAction::None, item_id: None };
    // The current code, the one before and the one after (a clock a little off, a slow typist),
    // with the login's user name, another or none: neither an update nor a new login.
    for code in [shown.code.clone(), at(frame.at_ms - 30_000), shown.next_code.clone().unwrap()] {
        for username in ["octocat", "someone", ""] {
            assert_eq!(advice(ask(username, &code)), nothing, "{username}: {code}");
        }
    }
    // Two steps away it is a password like any other; and a new password still updates the login.
    assert_eq!(advice(ask("octocat", &at(frame.at_ms + 60_000))), SaveAdvice { action: SaveAction::Update, item_id: Some(id) });
    assert_eq!(advice(ask("octocat", &at(frame.at_ms - 60_000))), SaveAdvice { action: SaveAction::Update, item_id: Some(id) });
    assert_eq!(advice(ask("octocat", "n3w-password")), SaveAdvice { action: SaveAction::Update, item_id: Some(id) });
    // An HOTP account's code: the counter's, the one before and the next.
    let bank = h.core.add_uri("otpauth://hotp/Bank:me?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&counter=1").unwrap();
    let vault = h.core.item_add(ItemDraft { totp_entry: Some(bank), ..login("Bank", "me", "https://bank.example/", Matching::Domain) }).unwrap();
    let at_bank = |username: &str, password: &str| QueryRequest { facts: facts("https://bank.example/otp"), ..ask(username, password) };
    for code in ["755224", "287082", "359152"] {
        assert_eq!(advice(at_bank("me", code)), nothing, "{code}");
    }
    assert_eq!(advice(at_bank("me", "969429")), SaveAdvice { action: SaveAction::Update, item_id: Some(vault) });
    // A login of another site, with its own code, does not guard this one's.
    assert_eq!(advice(ask("octocat", "287082")), SaveAdvice { action: SaveAction::Update, item_id: Some(id) });
}

#[tokio::test(start_paused = true)]
async fn frames_of_other_sites_and_plain_http_pages_are_refused_unless_allowed() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    h.core.item_add(login("Bank", "me", "https://bank.example/", Matching::Domain)).unwrap();
    let framed = FrameFacts { frame_url: "https://bank.example/login".into(), top_url: "https://evil.example/".into(), frame_chain: Vec::new() };
    let request = FillRequest { facts: framed.clone(), ..FillRequest::default() };
    assert_eq!(h.core.browser_fill(session, request.clone()).await, Err(Refusal::CrossOrigin), "never, by default");
    // A frame of the same site inside a frame of another site is cross-origin too.
    let nested = FrameFacts {
        frame_url: "https://bank.example/login".into(),
        top_url: "https://bank.example/".into(),
        frame_chain: vec!["https://ads.example/".into()],
    };
    assert_eq!(h.core.browser_fill(session, FillRequest { facts: nested, ..FillRequest::default() }).await, Err(Refusal::CrossOrigin));
    // An https login on an http page: the extension asks in the page, then says so.
    assert_eq!(h.core.browser_fill(session, fill_at("http://bank.example/login")).await, Err(Refusal::HttpDowngrade));
    let agreed = FillRequest { allow_http: true, ..fill_at("http://bank.example/login") };
    assert_eq!(h.core.browser_fill(session, agreed).await.unwrap().password.as_str(), PASSWORD);
    // With the frame rule set to ask, Lockra asks; a click is enough (the login does not ask).
    riskier(&h, |p| p.cross_origin_frames = Lenient::Known(FrameRule::Ask)).await;
    let core = h.core.clone();
    let fill = tokio::spawn(async move { core.browser_fill(session, request).await });
    settle().await;
    let waiting = h.core.state().browser.confirms[0].clone();
    assert_eq!((waiting.reasons.clone(), waiting.needs_proof), (vec![ConfirmReason::CrossOrigin], false));
    assert_eq!((waiting.frame_url.as_str(), waiting.top_url.as_deref()), ("https://bank.example/login", Some("https://evil.example/")));
    h.core.browser_confirm(waiting.id, None, None).await.unwrap();
    assert_eq!(fill.await.unwrap().unwrap().password.as_str(), PASSWORD);
}

#[tokio::test(start_paused = true)]
async fn a_login_that_asks_first_waits_for_proof_in_lockra() {
    let mut h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let bank = h.core.item_add(ItemDraft { protection: Protection::Confirm, ..login("Bank", "me", "https://bank.example/", Matching::Domain) }).unwrap();
    let _ = h.notices();
    let core = h.core.clone();
    let fill = tokio::spawn(async move { core.browser_fill(session, fill_at("https://bank.example/")).await });
    settle().await;
    let waiting = h.core.state().browser.confirms[0].clone();
    assert_eq!((waiting.item_id, waiting.needs_proof, waiting.title.as_str(), waiting.username.as_str()), (bank, true, "Bank", "me"));
    assert!(h.notices().contains(&Notice::BrowserConfirmRequested { id: waiting.id, browser: "chrome".into(), title: "Bank".into() }));
    assert!(!serde_json::to_string(&h.core.state()).unwrap().contains(PASSWORD), "the waiting request holds no secret");
    assert_eq!(code_err(h.core.browser_confirm(waiting.id, None, None).await), ErrorCode::BiometricUnavailable);
    h.core.browser_confirm(waiting.id, Some(pw(MASTER)), None).await.unwrap();
    assert_eq!(fill.await.unwrap().unwrap().password.as_str(), PASSWORD);
    // Denied, or not answered in time.
    let core = h.core.clone();
    let fill = tokio::spawn(async move { core.browser_fill(session, fill_at("https://bank.example/")).await });
    settle().await;
    h.core.browser_confirm_deny(h.core.state().browser.confirms[0].id).unwrap();
    assert_eq!(fill.await.unwrap(), Err(Refusal::Denied));
    let core = h.core.clone();
    let fill = tokio::spawn(async move { core.browser_fill(session, fill_at("https://bank.example/")).await });
    settle().await;
    let lapsed = h.core.state().browser.confirms[0].id;
    advance(Duration::from_secs(61)).await;
    assert_eq!(fill.await.unwrap(), Err(Refusal::Expired));
    assert!(h.core.state().browser.confirms.is_empty());
    assert_eq!(code_err(h.core.browser_confirm(lapsed, Some(pw(MASTER)), None).await), ErrorCode::BrowserPairExpired);
    // Copying its password from the popup waits the same way; its user name does not.
    h.core.browser_copy(session, CopyRequest { item_id: bank, part: CopyPart::Username }).await.unwrap();
    assert_eq!(h.clipboard.current().as_deref(), Some("me"));
    let core = h.core.clone();
    let copy = tokio::spawn(async move { core.browser_copy(session, CopyRequest { item_id: bank, part: CopyPart::Password }).await });
    settle().await;
    let waiting = h.core.state().browser.confirms[0].clone();
    assert_eq!((waiting.action, waiting.needs_proof), (crate::browser::ConfirmAction::Copy, true));
    h.core.browser_confirm(waiting.id, Some(pw(MASTER)), None).await.unwrap();
    copy.await.unwrap().unwrap();
    assert_eq!(h.clipboard.current().as_deref(), Some(PASSWORD));
}

#[tokio::test(start_paused = true)]
async fn a_fill_brings_the_linked_code_and_a_second_step_gets_it_for_five_minutes() {
    let mut h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let entry = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    let id = h.core.item_add(ItemDraft { totp_entry: Some(entry), ..login("GitHub", "octocat", "https://github.com/", Matching::Domain) }).unwrap();
    let _ = h.notices();
    let answer = h.core.browser_fill(session, fill_at("https://github.com/login")).await.unwrap();
    let code = answer.otp.clone().unwrap();
    assert_eq!(code.len(), 6);
    assert!(!answer.submit, "the policy does not submit by default");
    assert_eq!(h.clipboard.current().as_deref(), Some(code.as_str()), "copied as well");
    assert!(h.notices().contains(&Notice::ItemCopied { item_id: Some(id), part: crate::ui::CopiedPart::Totp, clear_after_s: Some(30) }));
    assert!(h.core.items_list().unwrap()[0].last_used_at_ms.is_some());
    // The next page of the sign-in, on the same site.
    let otp = OtpRequest { item_id: id, facts: facts("https://github.com/sessions/two-factor"), ..OtpRequest::default() };
    assert_eq!(h.core.browser_otp(session, otp.clone()).unwrap().code.len(), 6);
    let elsewhere = OtpRequest { item_id: id, facts: facts("https://example.org/2fa"), ..OtpRequest::default() };
    assert_eq!(h.core.browser_otp(session, elsewhere), Err(Refusal::NoMatch));
    // The user is at work meanwhile (a browser request never keeps the vault open by itself).
    advance(Duration::from_secs(4 * 60)).await;
    h.core.activity();
    advance(Duration::from_secs(60)).await;
    assert_eq!(h.core.browser_otp(session, otp.clone()), Err(Refusal::NoMatch), "five minutes at most");
    // The login's own choice wins over the policy.
    h.core.item_update(id, ItemPatch { autofill: Some(AutofillChoice { fill_otp: Tri::Off, ..AutofillChoice::default() }), ..ItemPatch::default() }).unwrap();
    let answer = h.core.browser_fill(session, fill_at("https://github.com/login")).await.unwrap();
    assert!(answer.otp.is_none());
    assert_eq!(h.core.browser_otp(session, otp), Err(Refusal::Protected));
    // Submitting is the login's or the policy's choice; copying the code can be turned off.
    riskier(&h, |p| {
        p.auto_submit = true;
        p.copy_otp = false;
    })
    .await;
    h.clipboard.put_text("something else");
    let answer = h.core.browser_fill(session, fill_at("https://github.com/login")).await.unwrap();
    assert!(answer.submit);
    assert_eq!(h.clipboard.current().as_deref(), Some("something else"), "not copied");
}

#[tokio::test(start_paused = true)]
async fn the_user_gets_a_login_s_code_where_the_login_is_offered_without_a_fill_before() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let entry = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    let id = h.core.item_add(ItemDraft { totp_entry: Some(entry), ..login("GitHub", "octocat", "https://github.com/", Matching::Domain) }).unwrap();
    let asked = |url: &str, trigger| OtpRequest { item_id: id, facts: facts(url), trigger: Some(trigger) };
    // No fill before: the next step's request gets nothing, the user's does.
    let next = OtpRequest { item_id: id, facts: facts("https://github.com/sessions/two-factor"), ..OtpRequest::default() };
    assert_eq!(h.core.browser_otp(session, next), Err(Refusal::NoMatch));
    for trigger in [Trigger::Popup, Trigger::InlineMenu, Trigger::Shortcut] {
        let answer = h.core.browser_otp(session, asked("https://github.com/sessions/two-factor", trigger)).unwrap();
        assert_eq!(answer.code.len(), 6, "{trigger:?}");
    }
    assert_eq!(h.core.state().browser.activity[0].event, ActivityEvent::OtpFilled);
    // Only where the login is offered, in the page or a frame of its own site.
    assert_eq!(h.core.browser_otp(session, asked("https://example.org/2fa", Trigger::Popup)), Err(Refusal::NoMatch));
    let framed = FrameFacts { frame_url: "https://github.com/2fa".into(), top_url: "https://example.org/".into(), frame_chain: Vec::new() };
    let in_frame = OtpRequest { item_id: id, facts: framed, trigger: Some(Trigger::Popup) };
    assert_eq!(h.core.browser_otp(session, in_frame), Err(Refusal::CrossOrigin));
    // What fills nothing by itself is no user's request.
    for trigger in [Trigger::PageLoad, Trigger::NextStep] {
        assert_eq!(h.core.browser_otp(session, asked("https://github.com/", trigger)), Err(Refusal::NoMatch), "{trigger:?}");
    }
    // The login's choice still counts.
    h.core.item_update(id, ItemPatch { autofill: Some(AutofillChoice { fill_otp: Tri::Off, ..AutofillChoice::default() }), ..ItemPatch::default() }).unwrap();
    assert_eq!(h.core.browser_otp(session, asked("https://github.com/", Trigger::Popup)), Err(Refusal::Protected));
}

#[tokio::test(start_paused = true)]
async fn a_login_is_deleted_from_the_extension_only_where_it_is_offered_and_after_proof_when_it_asks_first() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let github = h.core.item_add(login("GitHub", "octocat", "https://github.com/", Matching::Domain)).unwrap();
    let other = h.core.item_add(login("Example", "me", "https://example.org/", Matching::Domain)).unwrap();
    let bank = h.core.item_add(ItemDraft { protection: Protection::Confirm, ..login("Bank", "me", "https://bank.example/", Matching::Domain) }).unwrap();
    let delete = |item_id, facts: FrameFacts| DeleteRequest { item_id, facts };
    let ids = || h.core.items_list().unwrap().iter().map(|i| i.id).collect::<Vec<_>>();
    // Not offered on this page, or only in a frame of another site: nothing goes.
    assert_eq!(h.core.browser_delete(session, delete(other, facts("https://github.com/login"))).await, Err(Refusal::NoMatch));
    let framed = FrameFacts { frame_url: "https://github.com/login".into(), top_url: "https://example.org/".into(), frame_chain: Vec::new() };
    assert_eq!(h.core.browser_delete(session, delete(github, framed)).await, Err(Refusal::CrossOrigin));
    assert_eq!(ids().len(), 3);
    // Where it is offered: deleted, and the activity says so.
    h.core.browser_delete(session, delete(github, facts("https://github.com/login"))).await.unwrap();
    assert!(!ids().contains(&github));
    let done = &h.core.state().browser.activity[0];
    assert_eq!((done.event, done.title.as_deref(), done.site.as_str()), (ActivityEvent::Deleted, Some("GitHub"), "github.com"));
    assert_eq!(h.core.browser_delete(session, delete(github, facts("https://github.com/login"))).await, Err(Refusal::NoMatch), "gone");
    // A login that asks first waits for proof in Lockra, and a denial keeps it.
    let core = h.core.clone();
    let asking = tokio::spawn(async move { core.browser_delete(session, DeleteRequest { item_id: bank, facts: facts("https://bank.example/") }).await });
    settle().await;
    let waiting = h.core.state().browser.confirms[0].clone();
    assert_eq!((waiting.action, waiting.needs_proof, waiting.title.as_str()), (ConfirmAction::Delete, true, "Bank"));
    h.core.browser_confirm_deny(waiting.id).unwrap();
    assert_eq!(asking.await.unwrap(), Err(Refusal::Denied));
    assert!(ids().contains(&bank));
    let core = h.core.clone();
    let asking = tokio::spawn(async move { core.browser_delete(session, DeleteRequest { item_id: bank, facts: facts("https://bank.example/") }).await });
    settle().await;
    let waiting = h.core.state().browser.confirms[0].id;
    h.core.browser_confirm(waiting, Some(pw(MASTER)), None).await.unwrap();
    asking.await.unwrap().unwrap();
    assert!(!ids().contains(&bank));
    // Never on a never-fill site.
    h.core.site_rule_add(SiteRuleKind::NeverFill, &["example.org".into()]).unwrap();
    assert_eq!(h.core.browser_delete(session, delete(other, facts("https://example.org/"))).await, Err(Refusal::NeverFill));
    assert!(ids().contains(&other));
}

#[tokio::test(start_paused = true)]
async fn deleting_counts_with_the_secrets_so_a_session_cannot_empty_the_vault() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let ids: Vec<Uuid> =
        (0..=FILLS_PER_MINUTE).map(|n| h.core.item_add(login(&format!("Site {n}"), "me", "https://example.com/", Matching::Domain)).unwrap()).collect();
    for id in &ids[..FILLS_PER_MINUTE] {
        h.core.browser_delete(session, DeleteRequest { item_id: *id, facts: facts("https://example.com/") }).await.unwrap();
    }
    let last = DeleteRequest { item_id: ids[FILLS_PER_MINUTE], facts: facts("https://example.com/") };
    assert_eq!(h.core.browser_delete(session, last.clone()).await, Err(Refusal::RateLimited));
    assert_eq!(h.core.browser_delete(session, last).await, Err(Refusal::Paused), "and paused until the user resumes it");
    assert_eq!(h.core.items_list().unwrap().len(), 1, "the eleventh is kept");
}

#[tokio::test(start_paused = true)]
async fn a_code_is_linked_in_lockra_to_a_login_offered_for_the_frame() {
    // 2026-10-20: NDKEY's ADFS adapter asks for the one-time code in a password field
    // (`<input type="password" name="OTP">`). The save prompt offers 「关联验证码…」: Lockra comes up
    // on the login and the user picks the account there; the extension never sees the accounts.
    let mut h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let adfs = h.core.item_add(login("ADFS", "me", "https://sso.example.com/", Matching::Domain)).unwrap();
    let other = h.core.item_add(login("Example", "me", "https://example.org/", Matching::Domain)).unwrap();
    let hidden =
        h.core.item_add(ItemDraft { protection: Protection::NoExtension, ..login("Hidden", "me", "https://sso.example.com/", Matching::Domain) }).unwrap();
    let page = "https://sso.example.com/adfs/ls/";
    let link = |item_id, facts: FrameFacts| LinkCodeRequest { item_id, facts };
    let asked = || Notice::BrowserLinkCodeRequested { item_id: adfs, browser: "chrome".into(), title: "ADFS".into() };
    let rev = h.core.state().items_rev;
    let _ = h.notices();
    h.core.browser_link_code(session, link(adfs, facts(page))).unwrap();
    assert_eq!(h.notices(), [asked()]);
    assert_eq!(h.core.state().items_rev, rev, "nothing changes here: the user links the code in Lockra");
    // A login linked already asks the same: the user changes its code there.
    let entry = h.core.add_uri(&otpauth("ADFS", "me", SECRET)).unwrap();
    h.core.item_update(adfs, ItemPatch { totp_entry: Some(Some(entry)), ..ItemPatch::default() }).unwrap();
    let _ = h.notices();
    h.core.browser_link_code(session, link(adfs, facts(page))).unwrap();
    assert_eq!(h.notices(), [asked()]);
    // A login of another site or kept from the extension, a frame of another site, no page at all:
    // refused, and Lockra does not come up.
    assert_eq!(h.core.browser_link_code(session, link(other, facts(page))), Err(Refusal::NoMatch));
    assert_eq!(h.core.browser_link_code(session, link(hidden, facts(page))), Err(Refusal::NoMatch));
    let framed = FrameFacts { frame_url: page.into(), top_url: "https://evil.example/".into(), frame_chain: Vec::new() };
    assert_eq!(h.core.browser_link_code(session, link(adfs, framed)), Err(Refusal::CrossOrigin));
    let nested = FrameFacts { frame_url: page.into(), top_url: page.into(), frame_chain: vec!["https://ads.example/".into()] };
    assert_eq!(h.core.browser_link_code(session, link(adfs, nested)), Err(Refusal::CrossOrigin));
    assert_eq!(h.core.browser_link_code(session, link(adfs, facts("about:blank"))), Err(Refusal::Invalid));
    assert!(h.notices().is_empty());
}

#[tokio::test(start_paused = true)]
async fn asking_to_link_a_code_counts_as_a_query_and_waits_while_paused_or_locked() {
    let mut h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let id = h.core.item_add(login("GitHub", "octocat", "https://github.com/", Matching::Domain)).unwrap();
    let link = LinkCodeRequest { item_id: id, facts: facts("https://github.com/sessions/two-factor") };
    for _ in 0..QUERIES_PER_MINUTE {
        h.core.browser_query(session, QueryRequest { facts: facts("https://github.com/"), ..QueryRequest::default() }).unwrap();
    }
    let _ = h.notices();
    assert_eq!(h.core.browser_link_code(session, link.clone()), Err(Refusal::RateLimited));
    assert_eq!(h.core.browser_link_code(session, link.clone()), Err(Refusal::Paused));
    let asked = |n: &Notice| matches!(n, Notice::BrowserLinkCodeRequested { .. });
    assert!(!h.notices().iter().any(asked), "Lockra does not come up");
    h.core.browser_resume().unwrap();
    h.core.lock_vault();
    assert_eq!(h.core.browser_link_code(session, link), Err(Refusal::Locked));
    assert!(!h.notices().iter().any(asked));
}

#[tokio::test(start_paused = true)]
async fn the_next_step_of_a_sign_in_gets_the_password_once_and_moments_later() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let id = h.core.item_add(login("AWS", "root@example.com", "https://signin.aws.amazon.com/", Matching::Domain)).unwrap();
    let next = |url: &str| FillRequest { item_id: Some(id), trigger: Trigger::NextStep, ..fill_at(url) };
    let first = "https://signin.aws.amazon.com/signin";
    let second = "https://signin.aws.amazon.com/oauth";
    // Nothing the user started: no next step.
    assert_eq!(h.core.browser_fill(session, next(second)).await, Err(Refusal::NoMatch));
    // The user fills the first step; the next page of the same site gets the password, once.
    h.core.browser_fill(session, fill_at(first)).await.unwrap();
    assert_eq!(h.core.browser_fill(session, next(second)).await.unwrap().password.as_str(), PASSWORD);
    assert_eq!(h.core.browser_fill(session, next(second)).await, Err(Refusal::NoMatch), "once");
    // A next step is named, never guessed, and starts none of its own.
    h.core.browser_fill(session, fill_at(first)).await.unwrap();
    let guessed = FillRequest { trigger: Trigger::NextStep, ..fill_at(second) };
    assert_eq!(h.core.browser_fill(session, guessed).await, Err(Refusal::NoMatch));
    // Two minutes at most (the vault kept open meanwhile).
    h.core.browser_fill(session, fill_at(first)).await.unwrap();
    advance(Duration::from_secs(60)).await;
    h.core.activity();
    advance(Duration::from_secs(60)).await;
    assert_eq!(h.core.browser_fill(session, next(second)).await, Err(Refusal::NoMatch), "two minutes at most");
    // A fill as the page loads starts no next step.
    riskier(&h, |p| p.on_page_load = true).await;
    let on_load = FillRequest { trigger: Trigger::PageLoad, ..fill_at(first) };
    h.core.browser_fill(session, on_load).await.unwrap();
    assert_eq!(h.core.browser_fill(session, next(second)).await, Err(Refusal::NoMatch));
    // A login that asks first is filled only as the user asks, step by step.
    let bank = h.core.item_add(ItemDraft { protection: Protection::Confirm, ..login("Bank", "me", "https://bank.example/", Matching::Domain) }).unwrap();
    let core = h.core.clone();
    let fill = tokio::spawn(async move { core.browser_fill(session, fill_at("https://bank.example/")).await });
    settle().await;
    h.core.browser_confirm(h.core.state().browser.confirms[0].id, Some(pw(MASTER)), None).await.unwrap();
    fill.await.unwrap().unwrap();
    let bank_next = FillRequest { item_id: Some(bank), trigger: Trigger::NextStep, ..fill_at("https://bank.example/password") };
    assert_eq!(h.core.browser_fill(session, bank_next).await, Err(Refusal::Protected));
    // Nor in a frame of another site, where the user would be asked.
    riskier(&h, |p| p.cross_origin_frames = Lenient::Known(FrameRule::Ask)).await;
    h.core.browser_fill(session, fill_at(first)).await.unwrap();
    let framed = FillRequest {
        item_id: Some(id),
        trigger: Trigger::NextStep,
        facts: FrameFacts { frame_url: second.into(), top_url: "https://shop.example/".into(), frame_chain: Vec::new() },
        ..FillRequest::default()
    };
    assert_eq!(h.core.browser_fill(session, framed).await, Err(Refusal::CrossOrigin));
}

#[tokio::test(start_paused = true)]
async fn the_code_of_a_second_step_is_submitted_as_the_login_or_the_policy_says() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let entry = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    let id = h.core.item_add(ItemDraft { totp_entry: Some(entry), ..login("GitHub", "octocat", "https://github.com/", Matching::Domain) }).unwrap();
    let otp = OtpRequest { item_id: id, facts: facts("https://github.com/sessions/two-factor"), ..OtpRequest::default() };
    h.core.browser_fill(session, fill_at("https://github.com/login")).await.unwrap();
    assert!(!h.core.browser_otp(session, otp.clone()).unwrap().submit, "the policy does not submit by default");
    riskier(&h, |p| p.auto_submit = true).await;
    assert!(h.core.browser_otp(session, otp.clone()).unwrap().submit);
    h.core
        .item_update(id, ItemPatch { autofill: Some(AutofillChoice { auto_submit: Tri::Off, ..AutofillChoice::default() }), ..ItemPatch::default() })
        .unwrap();
    assert!(!h.core.browser_otp(session, otp).unwrap().submit, "the login's own choice wins");
}

#[tokio::test(start_paused = true)]
async fn page_load_fills_only_where_everything_is_plain() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let entry = h.core.add_uri(&otpauth("Shop", "me", SECRET)).unwrap();
    let shop = h.core.item_add(ItemDraft { totp_entry: Some(entry), ..login("Shop", "me", "https://shop.example/", Matching::Domain) }).unwrap();
    let on_load = |url: &str| FillRequest { trigger: Trigger::PageLoad, ..fill_at(url) };
    assert_eq!(h.core.browser_fill(session, on_load("https://shop.example/")).await, Err(Refusal::Protected), "off by default");
    riskier(&h, |p| {
        p.on_page_load = true;
        p.auto_submit = true;
    })
    .await;
    let answer = h.core.browser_fill(session, on_load("https://shop.example/")).await.unwrap();
    assert_eq!(answer.item_id, shop);
    assert!(answer.otp.is_none() && !answer.submit, "never a code, never a submit");
    // Not in a frame, not with two matches, not over http, not a login that asks or says no.
    let framed = FillRequest {
        trigger: Trigger::PageLoad,
        facts: FrameFacts {
            frame_url: "https://shop.example/".into(),
            top_url: "https://shop.example/".into(),
            frame_chain: vec!["https://shop.example/outer".into()],
        },
        ..FillRequest::default()
    };
    assert_eq!(h.core.browser_fill(session, framed).await, Err(Refusal::Protected));
    assert_eq!(h.core.browser_fill(session, FillRequest { allow_http: true, ..on_load("http://shop.example/") }).await, Err(Refusal::Protected));
    h.core
        .item_update(shop, ItemPatch { autofill: Some(AutofillChoice { on_page_load: Tri::Off, ..AutofillChoice::default() }), ..ItemPatch::default() })
        .unwrap();
    assert_eq!(h.core.browser_fill(session, on_load("https://shop.example/")).await, Err(Refusal::Protected));
    h.core.item_update(shop, ItemPatch { autofill: Some(AutofillChoice::default()), protection: Some(Protection::Confirm), ..ItemPatch::default() }).unwrap();
    assert_eq!(h.core.browser_fill(session, on_load("https://shop.example/")).await, Err(Refusal::Protected));
    h.core.item_update(shop, ItemPatch { protection: Some(Protection::Fill), ..ItemPatch::default() }).unwrap();
    h.core.item_add(login("Shop 2", "you", "https://shop.example/", Matching::Domain)).unwrap();
    assert_eq!(h.core.browser_fill(session, on_load("https://shop.example/")).await, Err(Refusal::Protected), "two matches");
    // An http login on an http page is plain enough.
    let plain = h.core.item_add(login("Router", "admin", "http://192.168.1.1/", Matching::Host)).unwrap();
    assert_eq!(h.core.browser_fill(session, on_load("http://192.168.1.1/")).await.unwrap().item_id, plain);
}

#[tokio::test(start_paused = true)]
async fn the_policy_asks_for_proof_only_to_let_the_extensions_do_more() {
    let h = Harness::unlocked().await;
    let defaults = h.core.state().browser.policy;
    assert_eq!(defaults, BrowserPolicy::default());
    assert!(defaults.fill_otp && defaults.copy_otp && !defaults.auto_submit && !defaults.confirm_fill && !defaults.on_page_load);
    assert_eq!(defaults.frames(), FrameRule::Never);
    // Less: no proof.
    let less = BrowserPolicy { fill_otp: false, copy_otp: false, confirm_fill: true, ..defaults.clone() };
    h.core.browser_set_policy(less.clone(), None, None).await.unwrap();
    assert_eq!(h.core.state().browser.policy, less);
    // More, one change at a time: each needs proof.
    for more in [
        BrowserPolicy { on_page_load: true, ..less.clone() },
        BrowserPolicy { auto_submit: true, ..less.clone() },
        BrowserPolicy { fill_otp: true, ..less.clone() },
        BrowserPolicy { confirm_fill: false, ..less.clone() },
        BrowserPolicy { cross_origin_frames: Lenient::Known(FrameRule::Ask), ..less.clone() },
    ] {
        assert_eq!(code_err(h.core.browser_set_policy(more.clone(), None, None).await), ErrorCode::BiometricUnavailable, "{more:?}");
        assert_eq!(h.core.state().browser.policy, less);
    }
    let more = BrowserPolicy { on_page_load: true, ..less };
    h.core.browser_set_policy(more.clone(), Some(pw(MASTER)), None).await.unwrap();
    // Kept in the vault, not in settings.json.
    let restarted = h.restart();
    restarted.unlock(pw(MASTER)).await.unwrap();
    assert_eq!(restarted.state().browser.policy, more);
    assert!(!fs::read_to_string(h.dir.path().join("config").join("settings.json")).unwrap_or_default().contains("on_page_load"));
}

#[tokio::test(start_paused = true)]
async fn an_extension_that_asks_too_often_is_paused_until_resumed() {
    let mut h = Harness::unlocked().await;
    let (pairing, session) = paired(&h).await;
    h.core.item_add(login("GitHub", "octocat", "https://github.com/", Matching::Domain)).unwrap();
    for _ in 0..FILLS_PER_MINUTE {
        h.core.browser_fill(session, fill_at("https://github.com/")).await.unwrap();
    }
    let _ = h.notices();
    assert_eq!(h.core.browser_fill(session, fill_at("https://github.com/")).await, Err(Refusal::RateLimited));
    assert!(h.notices().contains(&Notice::BrowserPaused { pairing, browser: "chrome".into(), label: "Work".into() }));
    let state = h.core.state();
    assert!(state.browser.paired[0].paused);
    assert_eq!(state.browser.activity[0].event, ActivityEvent::Paused);
    // Paused: nothing but the status, a minute later too, and a new session of it as well.
    advance(Duration::from_secs(61)).await;
    assert_eq!(h.core.browser_fill(session, fill_at("https://github.com/")).await, Err(Refusal::Paused));
    let query = QueryRequest { facts: facts("https://github.com/"), ..QueryRequest::default() };
    assert_eq!(h.core.browser_query(session, query.clone()), Err(Refusal::Paused));
    assert!(h.core.browser_status(session).unwrap().paused);
    let other = h.core.browser_session_open(pairing, chrome()).unwrap();
    assert_eq!(h.core.browser_query(other, query.clone()), Err(Refusal::Paused));
    h.core.browser_resume().unwrap();
    assert!(!h.core.state().browser.paired[0].paused);
    h.core.browser_fill(session, fill_at("https://github.com/")).await.unwrap();
    // Queries have their own, larger budget.
    h.core.browser_resume().unwrap();
    advance(Duration::from_secs(61)).await;
    for _ in 0..QUERIES_PER_MINUTE {
        h.core.browser_query(other, query.clone()).unwrap();
    }
    assert_eq!(h.core.browser_query(other, query), Err(Refusal::RateLimited));
}

#[tokio::test(start_paused = true)]
async fn secrets_of_too_many_sites_pause_the_extension() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    for n in 0..=SITES_PER_TEN_MINUTES {
        h.core.item_add(login(&format!("Site {n}"), "me", &format!("https://site{n}.example/"), Matching::Domain)).unwrap();
    }
    for n in 0..SITES_PER_TEN_MINUTES {
        // Spread over the minutes, so that only the sites count; the user is at work meanwhile.
        advance(Duration::from_secs(20)).await;
        h.core.activity();
        h.core.browser_fill(session, fill_at(&format!("https://site{n}.example/"))).await.unwrap();
    }
    advance(Duration::from_secs(20)).await;
    assert_eq!(h.core.browser_fill(session, fill_at(&format!("https://site{SITES_PER_TEN_MINUTES}.example/"))).await, Err(Refusal::RateLimited));
}

#[tokio::test(start_paused = true)]
async fn a_busy_extension_does_not_keep_the_vault_open() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let id = h.core.item_add(login("GitHub", "octocat", "https://github.com/", Matching::Domain)).unwrap();
    let minutes = u64::from(h.core.state().settings.auto_lock_minutes);
    for minute in 0..minutes {
        advance(Duration::from_secs(60)).await;
        let _ = h.core.browser_query(session, QueryRequest { facts: facts("https://github.com/"), ..QueryRequest::default() });
        let _ = h.core.browser_fill(session, fill_at("https://github.com/")).await;
        let _ = h.core.browser_link_code(session, LinkCodeRequest { item_id: id, facts: facts("https://github.com/") });
        let save = SaveRequest { facts: facts("https://new.example/"), username: "me".into(), password: pw("p"), ..SaveRequest::default() };
        let _ = h.core.browser_save(session, save);
        let rule = SiteRuleRequest { kind: Some(SiteRuleKind::NeverSave), facts: facts("https://other.example/") };
        let _ = h.core.browser_site_rule(session, rule);
        let switch = SiteMatchingRequest { facts: facts("https://git.other.example/"), host: minute % 2 == 0 };
        let _ = h.core.browser_site_matching(session, switch);
    }
    advance(Duration::from_secs(1)).await;
    assert_eq!(h.core.state().phase, Phase::Locked, "locked after the idle time, the requests notwithstanding");
}

#[tokio::test(start_paused = true)]
async fn a_submitted_form_is_offered_for_saving_or_updating_and_saved_from_the_page() {
    let mut h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let ask = |username: &str, password: &str| QueryRequest {
        facts: facts("https://login.example.com/sign-in"),
        purpose: QueryPurpose::Save,
        username: username.into(),
        password: pw(password),
    };
    let advice = |h: &Harness, q: QueryRequest| h.core.browser_query(session, q).unwrap().save.unwrap();
    assert_eq!(advice(&h, ask("alice", "s3cret")).action, SaveAction::Save);
    let _ = h.notices();
    let saved = SaveRequest { facts: facts("https://login.example.com/sign-in"), username: "alice".into(), password: pw("s3cret"), ..SaveRequest::default() };
    let id = h.core.browser_save(session, saved).unwrap();
    let item = h.core.items_list().unwrap().into_iter().find(|i| i.id == id).unwrap();
    assert_eq!((item.title.as_str(), item.username.as_str()), ("example.com", "alice"));
    assert_eq!(item.urls.iter().map(|u| (u.url.as_str(), u.matching)).collect::<Vec<_>>(), [("https://login.example.com/", Matching::Domain)]);
    assert!(h.notices().contains(&Notice::ItemSaved { item_id: id, title: "example.com".into(), updated: false }));
    assert_eq!(advice(&h, ask("alice", "s3cret")).action, SaveAction::None, "saved already");
    assert_eq!(advice(&h, ask("ALICE", "s3cret")).action, SaveAction::None, "user names compare without case");
    let update = advice(&h, ask("alice", "n3w"));
    assert_eq!((update.action, update.item_id), (SaveAction::Update, Some(id)));
    assert_eq!(advice(&h, ask("bob", "other")).action, SaveAction::Save);
    // The change of password keeps the old one in the history.
    let request = UpdateRequest { item_id: id, facts: facts("https://login.example.com/"), password: pw("n3w") };
    h.core.browser_update(session, request).unwrap();
    let revealed = h.core.item_reveal(id, RevealParts { password: true, history: true, ..RevealParts::default() }, None, None).await.unwrap();
    assert_eq!(revealed.password.as_deref(), Some("n3w"));
    assert!(revealed.history.unwrap().iter().any(|p| p.value == "s3cret"));
    // Only a login that matches the page is updated from it.
    let elsewhere = UpdateRequest { item_id: id, facts: facts("https://example.org/"), password: pw("x") };
    assert_eq!(h.core.browser_update(session, elsewhere), Err(Refusal::NoMatch));
    // "Not on this site".
    let rule = SiteRuleRequest { kind: Some(SiteRuleKind::NeverSave), facts: facts("https://www.example.com/") };
    h.core.browser_site_rule(session, rule).unwrap();
    assert_eq!(advice(&h, ask("carol", "pw")).action, SaveAction::None);
    let blocked = SaveRequest { facts: facts("https://login.example.com/"), username: "carol".into(), password: pw("pw"), ..SaveRequest::default() };
    assert_eq!(h.core.browser_save(session, blocked), Err(Refusal::NeverSave));
    let equivalent = SiteRuleRequest { kind: Some(SiteRuleKind::Equivalent), facts: facts("https://example.com/") };
    assert_eq!(h.core.browser_site_rule(session, equivalent), Err(Refusal::Invalid), "only the two lists");
    assert_eq!(h.core.state().site_rules.iter().map(|r| r.domains.clone()).collect::<Vec<_>>(), [vec!["example.com".to_owned()]]);
}

#[tokio::test(start_paused = true)]
async fn a_sign_up_form_gets_a_password_to_its_rules_saved_at_once() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let request = GenerateRequest {
        facts: facts("https://new.example/signup"),
        rules: Some("minlength: 30; required: digit; allowed: lower;".into()),
        save: Some(GenerateSave { username: Some("newbie".into()), title: String::new() }),
        ..GenerateRequest::default()
    };
    let made = h.core.browser_generate(session, request).unwrap();
    assert!(made.password.chars().count() >= 30 && made.password.chars().any(|c| c.is_ascii_digit()));
    assert!(made.password.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()), "{}", made.password.as_str());
    assert_eq!(made.username, "newbie");
    let id = made.item_id.unwrap();
    let revealed = h.core.item_reveal(id, RevealParts { password: true, ..RevealParts::default() }, None, None).await.unwrap();
    assert_eq!(revealed.password.as_deref(), Some(made.password.as_str()));
    assert_eq!(h.core.state().generator.history_count, 1, "in the generator's history");
    // Without saving: the password only.
    let plain = h.core.browser_generate(session, GenerateRequest { facts: facts("https://new.example/"), ..GenerateRequest::default() }).unwrap();
    assert!(plain.item_id.is_none() && plain.password.chars().count() >= 20);
}

#[tokio::test(start_paused = true)]
async fn the_sign_up_menu_copies_the_password_lockra_made_for_its_site_and_nothing_else() {
    // 2026-10-16, the user: the menu should copy the password it shows. The app puts it on the
    // clipboard as it does a login's, cleared after the setting's time.
    let mut h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let made = h.core.browser_generate(session, GenerateRequest { facts: facts("https://new.example/signup"), ..GenerateRequest::default() }).unwrap();
    let _ = h.notices();
    let copy = |url: &str, password: &str| CopyGeneratedRequest { facts: facts(url), password: pw(password) };
    h.core.browser_copy_generated(session, copy("https://new.example/signup", made.password.as_str())).unwrap();
    assert_eq!(h.clipboard.current().as_deref(), Some(made.password.as_str()));
    assert!(h.notices().contains(&Notice::ItemCopied { item_id: None, part: crate::ui::CopiedPart::Password, clear_after_s: Some(30) }));
    // Another site's page, or a password Lockra did not make: the app copies nothing.
    h.clipboard.put_text("unchanged");
    assert_eq!(h.core.browser_copy_generated(session, copy("https://other.example/", made.password.as_str())), Err(Refusal::NotFound));
    assert_eq!(h.core.browser_copy_generated(session, copy("https://new.example/", "typed-by-the-page")), Err(Refusal::NotFound));
    assert_eq!(h.clipboard.current().as_deref(), Some("unchanged"));
}

#[tokio::test(start_paused = true)]
async fn the_popup_searches_every_login_and_tells_those_of_the_page() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let github = h.core.item_add(login("GitHub", "octocat", "https://github.com/", Matching::Domain)).unwrap();
    let gitlab = h.core.item_add(login("GitLab", "tanuki", "https://gitlab.com/", Matching::Domain)).unwrap();
    h.core.item_add(ItemDraft { protection: Protection::NoExtension, ..login("Git server", "root", "https://git.example/", Matching::Domain) }).unwrap();
    let found = h.core.browser_search(session, SearchRequest { text: " GIT ".into(), top_url: "https://github.com/x".into() }).unwrap();
    let ids: Vec<(Uuid, bool)> = found.iter().map(|i| (i.item_id, i.matches)).collect();
    assert_eq!(ids, [(github, true), (gitlab, false)], "never a login kept from the extension");
    assert!(h.core.browser_search(session, SearchRequest { text: "  ".into(), top_url: String::new() }).unwrap().is_empty());
    assert_eq!(h.core.browser_search(session, SearchRequest { text: "tanuki".into(), top_url: String::new() }).unwrap()[0].item_id, gitlab);
}

#[tokio::test(start_paused = true)]
async fn the_paired_list_tells_the_version_each_extension_said_it_was() {
    let h = Harness::unlocked().await;
    let (pairing, session) = paired(&h).await;
    let version = |h: &Harness| h.core.state().browser.paired[0].version.clone();
    assert_eq!(version(&h).as_deref(), Some("0.10.0"));
    // It stays once the session ends, and the next connection's replaces it.
    h.core.browser_session_close(session);
    assert_eq!(version(&h).as_deref(), Some("0.10.0"));
    h.core.browser_session_open(pairing, ClientInfo { version: "0.10.2".into(), ..chrome() }).unwrap();
    assert_eq!(version(&h).as_deref(), Some("0.10.2"));
    // An extension that names none has none.
    h.core.browser_session_open(pairing, ClientInfo { version: " \n".into(), ..chrome() }).unwrap();
    assert_eq!(version(&h), None);
    // Forgotten with the pairing.
    h.core.browser_session_open(pairing, chrome()).unwrap();
    h.core.browser_unpair(pairing).unwrap();
    let again = paired_again(&h).await;
    assert_ne!(again, pairing);
    assert_eq!(h.core.state().browser.paired[0].version.as_deref(), Some("0.10.0"));
}

/// A second pairing of the same extension, without a session.
async fn paired_again(h: &Harness) -> Uuid {
    let core = h.core.clone();
    let pairing = tokio::spawn(async move { core.browser_pair(chrome(), KEY, "123456".into()).await });
    settle().await;
    let request = h.core.state().browser.pairing[0].id;
    h.core.browser_pair_approve(request, Some(pw(MASTER)), None).await.unwrap();
    let pairing = pairing.await.unwrap().unwrap();
    assert_eq!(h.core.state().browser.paired[0].version, None, "none before it connects");
    h.core.browser_session_open(pairing, chrome()).unwrap();
    pairing
}

#[tokio::test(start_paused = true)]
async fn removing_a_pairing_ends_its_sessions_at_once() {
    let h = Harness::unlocked().await;
    let (pairing, session) = paired(&h).await;
    assert!(h.core.state().browser.paired[0].connected);
    assert_eq!(h.core.state().browser.paired[0].page_access, Some(true));
    h.core.browser_unpair(pairing).unwrap();
    assert!(!h.core.browser_session_alive(session));
    assert_eq!(h.core.browser_status(session), Err(Refusal::NotPaired));
    assert_eq!(h.core.browser_peer(pairing), None);
    assert_eq!(h.core.browser_session_open(pairing, chrome()), Err(Refusal::NotPaired));
    assert_eq!(h.core.state().browser.activity[0].event, ActivityEvent::Unpaired);
    assert_eq!(code_err(h.core.browser_unpair(pairing)), ErrorCode::BrowserNotPaired);
    // A session that ends by itself is no longer connected.
    let (pairing, session) = paired(&h).await;
    h.core.browser_session_close(session);
    assert!(!h.core.state().browser.paired.iter().find(|p| p.id == pairing).unwrap().connected);
}

#[tokio::test(start_paused = true)]
async fn an_unlock_request_comes_through_at_most_every_ten_seconds_and_only_when_locked() {
    let mut h = Harness::unlocked().await;
    assert!(!h.core.browser_unlock_request(&chrome()), "unlocked already");
    h.core.lock_vault();
    let _ = h.notices();
    assert!(h.core.browser_unlock_request(&chrome()));
    assert!(h.notices().contains(&Notice::BrowserUnlockRequested { browser: "chrome".into() }));
    assert!(!h.core.browser_unlock_request(&chrome()));
    advance(Duration::from_secs(10)).await;
    assert!(h.core.browser_unlock_request(&chrome()));
}

#[tokio::test(start_paused = true)]
async fn the_integration_switch_is_the_shells_and_shows_while_locked() {
    let h = Harness::unlocked().await;
    assert!(!h.core.browser_enabled());
    h.core.browser_set_enabled(true).unwrap();
    assert!(h.core.browser_enabled() && h.core.state().browser.enabled);
    // The webview's settings leave it as it is.
    let settings = Settings { browser_integration: false, ..h.core.state().settings };
    h.core.set_settings(settings).unwrap();
    assert!(h.core.browser_enabled());
    h.core.lock_vault();
    let state = h.core.state();
    assert!(state.browser.available && state.browser.enabled, "readable locked: Lockra listens before unlocking");
    let restarted = h.restart();
    assert!(restarted.browser_enabled(), "kept in settings.json");
    h.core.browser_set_enabled(false).unwrap();
    assert!(!h.core.browser_enabled());
}

#[tokio::test(start_paused = true)]
async fn the_phone_and_a_build_without_password_management_offer_no_browser() {
    let off = harness_with_features(Features { passwords: false });
    off.core.create_vault(pw(MASTER)).await.unwrap();
    assert!(!off.core.state().browser.available);
    assert!(off.core.browser_static_secret().is_none());
    assert_eq!(code_err(off.core.browser_set_enabled(true)), ErrorCode::FeatureUnavailable);
    assert_eq!(off.core.browser_pair(chrome(), KEY, "1".into()).await, Err(Refusal::Unavailable));
    let dir = tempfile::tempdir().unwrap();
    let config = CoreConfig {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        app_version: "0.1.0-test".into(),
        kdf: KdfCost::FAST_INSECURE,
        platform: Platform::Android,
        features: Features { passwords: true },
    };
    let ports = Ports {
        secrets: Arc::new(FakeKeychain::default()),
        clipboard: Arc::new(FakeClipboard::default()),
        clock: Arc::new(FakeClock::new(T0)),
        updater: Arc::new(FakeUpdater::default()),
        sync: Arc::new(FakeTransport::default()),
        biometrics: Arc::new(FakeBiometrics::default()),
    };
    let phone = Core::start(config, ports);
    phone.create_vault(pw(MASTER)).await.unwrap();
    assert!(!phone.state().browser.available);
    assert!(phone.browser_static_secret().is_none());
    assert_eq!(code_err(phone.browser_set_enabled(true)), ErrorCode::BrowserUnavailable);
    assert_eq!(code_err(phone.browser_resume()), ErrorCode::BrowserUnavailable);
    assert_eq!(code_err(phone.browser_set_policy(BrowserPolicy::default(), None, None).await), ErrorCode::BrowserUnavailable);
    assert_eq!(phone.browser_session_open(Uuid::new_v4(), chrome()), Err(Refusal::Unavailable));
}

#[tokio::test(start_paused = true)]
async fn what_an_extension_says_of_itself_is_cut_to_size() {
    let h = Harness::unlocked().await;
    let noisy = ClientInfo { browser: "Chrome\u{7}".repeat(20), label: "x".repeat(500), extension: "\n".into(), version: "1".repeat(99), page_access: false };
    let core = h.core.clone();
    let request = tokio::spawn(async move { core.browser_pair(noisy, KEY, "1".into()).await });
    settle().await;
    let shown = h.core.state().browser.pairing[0].clone();
    assert_eq!((shown.browser.chars().count(), shown.label.chars().count(), shown.extension.as_str()), (32, 64, ""));
    assert!(!shown.browser.contains('\u{7}'));
    request.abort();
}

#[tokio::test(start_paused = true)]
async fn a_fill_leaves_no_secret_in_the_state() {
    let h = Harness::unlocked().await;
    let (_, session) = paired(&h).await;
    let entry = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    h.core
        .item_add(ItemDraft {
            totp_entry: Some(entry),
            fields: vec![crate::logins::FieldChange {
                label: Some("PIN".into()),
                value: SecretChange::Set { value: pw("pin-4242") },
                hidden: Some(true),
                page_field: Some("pin".into()),
                ..crate::logins::FieldChange::default()
            }],
            ..login("GitHub", "octocat", "https://github.com/", Matching::Domain)
        })
        .unwrap();
    let answer = h.core.browser_fill(session, fill_at("https://github.com/")).await.unwrap();
    assert_eq!(answer.fields.iter().map(|f| (f.page_field.as_str(), f.value.as_str())).collect::<Vec<_>>(), [("pin", "pin-4242")]);
    let state = serde_json::to_string(&h.core.state()).unwrap();
    for secret in [PASSWORD, "pin-4242", answer.otp.as_deref().unwrap()] {
        assert!(!state.contains(secret), "{secret} in the state");
    }
}
