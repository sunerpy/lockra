use lockra_sync::Hlc;
use tokio::time::Instant;

use super::*;
use crate::item::{SiteRule, UrlRule};

fn url(text: &str) -> WebUrl {
    WebUrl::parse(text).unwrap()
}

#[test]
fn the_policy_reads_leniently_and_tells_a_riskier_one() {
    let read: BrowserPolicy = serde_json::from_str("{}").unwrap();
    assert_eq!(read, BrowserPolicy::default(), "what is missing is the default");
    let later: BrowserPolicy = serde_json::from_str(r#"{"cross_origin_frames":"same_brand"}"#).unwrap();
    assert_eq!(later.frames(), FrameRule::Never, "a rule of a later version fills nothing");
    assert!(serde_json::to_string(&later).unwrap().contains("same_brand"), "kept as written");
    let base = BrowserPolicy::default();
    assert!(!base.riskier(&base));
    assert!(!base.riskier(&BrowserPolicy { copy_otp: false, fill_otp: false, confirm_fill: true, ..base.clone() }));
    assert!(base.riskier(&BrowserPolicy { on_page_load: true, ..base.clone() }));
    assert!(base.riskier(&BrowserPolicy { auto_submit: true, ..base.clone() }));
    assert!(base.riskier(&BrowserPolicy { cross_origin_frames: Lenient::Known(FrameRule::Ask), ..base.clone() }));
    let careful = BrowserPolicy { confirm_fill: true, fill_otp: false, ..base.clone() };
    assert!(careful.riskier(&base), "no confirmation, codes filled again");
    assert!(!careful.riskier(&BrowserPolicy { copy_otp: true, ..careful.clone() }), "copying the code is no more than the user gets anyway");
    let ports = BrowserPolicy { match_port: true, ..base.clone() };
    assert!(!base.riskier(&ports) && !ports.riskier(&base), "the port narrows the matches, and without it they are the default's");
    let host = BrowserPolicy { match_host: true, ..base.clone() };
    assert!(!base.riskier(&host) && !host.riskier(&base), "the host narrows the matches, and the site is the default's");
}

#[test]
fn the_vault_part_keeps_its_key_out_of_debug_output_and_reads_back() {
    let local = BrowserLocal {
        key: Some(StaticKey(Zeroizing::new([5; 32]))),
        paired: vec![PairedExtension {
            id: Uuid::from_u128(1),
            browser: "firefox".into(),
            label: String::new(),
            extension: "lockra@firlab.app".into(),
            public_key: [9; 32],
            paired_at_ms: 1,
            last_seen_ms: None,
        }],
        policy: BrowserPolicy::default(),
    };
    assert!(!format!("{local:?}").contains("5, 5"), "the key is not in debug output");
    let text = serde_json::to_string(&local).unwrap();
    assert!(!text.contains("policy"), "a default policy is not written");
    assert_eq!(serde_json::from_str::<BrowserLocal>(&text).unwrap(), local);
    assert!(BrowserLocal::default().is_empty() && !local.is_empty());
    assert!(serde_json::from_str::<StaticKey>(r#""AAAA""#).is_err(), "a key is 32 bytes");
}

#[test]
fn site_lists_cover_a_host_and_its_subdomains_only() {
    let mut data = VaultData::new();
    data.site_rules.push(SiteRule {
        id: Uuid::from_u128(1),
        kind: Lenient::Known(SiteRuleKind::NeverFill),
        domains: vec!["bank.example".into(), "例え.jp".into()],
        stamp: Default::default(),
        extra: Default::default(),
    });
    assert!(listed(&data, SiteRuleKind::NeverFill, &url("https://bank.example/")));
    assert!(listed(&data, SiteRuleKind::NeverFill, &url("https://www.bank.example/login")));
    assert!(listed(&data, SiteRuleKind::NeverFill, &url("https://xn--r8jz45g.jp/")), "the same host however written");
    assert!(!listed(&data, SiteRuleKind::NeverFill, &url("https://notbank.example/")));
    assert!(!listed(&data, SiteRuleKind::NeverFill, &url("https://example/")));
    assert!(!listed(&data, SiteRuleKind::NeverSave, &url("https://bank.example/")), "another list");
}

#[test]
fn a_frame_of_another_site_anywhere_in_the_chain_is_cross_origin() {
    let top = url("https://shop.example/");
    assert!(!cross_origin(&url("https://pay.shop.example/"), &top, &[]));
    assert!(cross_origin(&url("https://pay.example/"), &top, &[]));
    assert!(cross_origin(&url("https://pay.shop.example/"), &top, &[url("https://ads.example/")]));
}

#[test]
fn matches_offer_the_likeliest_login_first() {
    let mut data = VaultData::new();
    let mut login = |title: &str, favorite: bool, used: Option<u64>, protection: Protection| {
        let mut item = VaultItem::login(Uuid::new_v4(), 1, Default::default(), 1);
        item.title = title.into();
        item.favorite = favorite;
        item.last_used_at_ms = used;
        item.protection = Lenient::Known(protection);
        item.urls = vec![UrlRule { url: "https://example.com/".into(), matching: Lenient::Known(Matching::Domain), extra: Default::default() }];
        data.items.push(item.clone());
        item.id
    };
    let old = login("b old", false, Some(1), Protection::Fill);
    let recent = login("c recent", false, Some(9), Protection::Fill);
    let pinned = login("z pinned", true, None, Protection::Confirm);
    let never = login("a never", false, None, Protection::NoExtension);
    let (level, found) = matches(&data, &url("https://www.example.com/"), &BrowserPolicy::default());
    assert_eq!(level, Some(MatchLevel::Domain));
    assert_eq!(found.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [pinned, recent, old]);
    assert!(!found.iter().any(|(id, _)| *id == never));
    assert_eq!(matches(&data, &url("https://example.org/"), &BrowserPolicy::default()), (None, Vec::new()));
}

fn rule(kind: SiteRuleKind, domain: &str, stamp: u64) -> SiteRule {
    SiteRule {
        id: SiteRule::single_id(kind, domain),
        kind: Lenient::Known(kind),
        domains: vec![domain.into()],
        stamp: Hlc::at(stamp),
        extra: Default::default(),
    }
}

#[test]
fn the_domain_rule_reaches_as_far_as_the_closest_site_rule_or_the_policy_says() {
    let mut data = VaultData::new();
    let site = BrowserPolicy::default();
    let ports = BrowserPolicy { match_port: true, ..site.clone() };
    let host = BrowserPolicy { match_host: true, ..site.clone() };
    let page = url("https://git.example.cn/users/sign_in");
    // No rule: the policy's default, with the port as it says; the host has its port anyway.
    assert_eq!(domain_scope(&data, &site, &page), DomainScope::Site);
    assert_eq!(domain_scope(&data, &ports, &page), DomainScope::SitePort);
    assert_eq!(domain_scope(&data, &host, &page), DomainScope::Host);
    assert_eq!(domain_scope(&data, &BrowserPolicy { match_port: true, ..host.clone() }, &page), DomainScope::Host);
    // A rule about a domain covers it and its subdomains, however written, and nothing else.
    data.site_rules.push(rule(SiteRuleKind::MatchHost, "example.cn", 1));
    data.site_rules.push(rule(SiteRuleKind::MatchHost, "例え.jp", 1));
    assert_eq!(domain_scope(&data, &site, &page), DomainScope::Host);
    assert_eq!(domain_scope(&data, &ports, &url("https://example.cn/")), DomainScope::Host);
    assert_eq!(domain_scope(&data, &site, &url("https://www.xn--r8jz45g.jp/")), DomainScope::Host);
    assert_eq!(domain_scope(&data, &site, &url("https://notexample.cn/")), DomainScope::Site);
    assert_eq!(domain_scope(&data, &ports, &url("https://example.com/")), DomainScope::SitePort, "another site");
    // The rule of the longest domain wins, whatever their age: one host of the site by the domain.
    data.site_rules.push(rule(SiteRuleKind::MatchDomain, "git.example.cn", 0));
    assert_eq!(domain_scope(&data, &site, &page), DomainScope::Site);
    assert_eq!(domain_scope(&data, &ports, &page), DomainScope::SitePort);
    assert_eq!(domain_scope(&data, &site, &url("https://wiki.example.cn/")), DomainScope::Host);
    assert_eq!(domain_scope(&data, &site, &url("https://a.git.example.cn/")), DomainScope::Site);
    // A domain under both kinds, as two devices may leave it until they sync: the later rule.
    data.site_rules.push(rule(SiteRuleKind::MatchHost, "git.example.cn", 2));
    assert_eq!(domain_scope(&data, &site, &page), DomainScope::Host);
    data.site_rules.retain(|r| r.stamp != Hlc::at(2));
    data.site_rules.push(rule(SiteRuleKind::MatchHost, "git.example.cn", 0));
    assert_eq!(domain_scope(&data, &site, &page), DomainScope::Host, "the same stamp: the full host");
    // The other kinds, and a kind of a later version, say nothing of it.
    let mut later = rule(SiteRuleKind::MatchHost, "example.com", 9);
    later.kind = Lenient::Unknown("match_path".into());
    data.site_rules.push(later);
    data.site_rules.push(rule(SiteRuleKind::NeverFill, "example.com", 9));
    data.site_rules.push(SiteRule { domains: vec!["example.com".into(), "example.net".into()], ..rule(SiteRuleKind::Equivalent, "example.com", 9) });
    assert_eq!(domain_scope(&data, &site, &url("https://www.example.com/")), DomainScope::Site);
    assert_eq!(domain_scope(&data, &host, &url("https://www.example.com/")), DomainScope::Host);
}

#[test]
fn a_session_counts_its_requests_by_the_minute_and_its_sites_by_ten() {
    let start = Instant::now();
    let mut session = SessionState::new(Uuid::nil(), ClientInfo::default());
    for _ in 0..QUERIES_PER_MINUTE {
        assert!(session.query(start));
    }
    assert!(!session.query(start));
    assert!(session.query(start + Duration::from_secs(60)), "a minute later the count starts again");
    for _ in 0..FILLS_PER_MINUTE {
        assert!(session.secret("a.example", start));
    }
    assert!(!session.secret("a.example", start));
    let mut sites = SessionState::new(Uuid::nil(), ClientInfo::default());
    for n in 0..SITES_PER_TEN_MINUTES {
        assert!(sites.secret(&format!("s{n}.example"), start + Duration::from_secs(61 * n as u64 / 4)));
    }
    let later = start + Duration::from_secs(300);
    assert!(!sites.secret("one-more.example", later));
    assert!(sites.secret("again.example", start + Duration::from_secs(1200)), "ten minutes later the sites start again");
    sites.filled(Uuid::from_u128(1), "a.example", start);
    assert!(sites.filled_lately(Uuid::from_u128(1), "a.example", start + OTP_FOLLOW_UP - Duration::from_secs(1)));
    assert!(!sites.filled_lately(Uuid::from_u128(1), "a.example", start + OTP_FOLLOW_UP));
    assert!(!sites.filled_lately(Uuid::from_u128(1), "b.example", start));
}

#[test]
fn what_an_extension_says_is_cleaned() {
    let client =
        ClientInfo { browser: " Chrome\n".into(), label: "\u{1b}[31mWork".into(), extension: "x".repeat(300), version: "1.0".into(), page_access: true };
    let cleaned = client.cleaned();
    assert_eq!((cleaned.browser.as_str(), cleaned.label.as_str(), cleaned.extension.len()), ("chrome", "[31mWork", 128));
}

#[test]
fn the_activity_list_keeps_the_newest_hundred() {
    let mut runtime = BrowserRuntime::default();
    for n in 0..150 {
        runtime.record(Activity {
            at_ms: n,
            browser: "chrome".into(),
            label: String::new(),
            site: String::new(),
            event: ActivityEvent::Filled,
            refusal: None,
            item_id: None,
            title: None,
        });
    }
    assert_eq!(runtime.activity.len(), MAX_ACTIVITY);
    assert_eq!(runtime.activity.front().map(|a| a.at_ms), Some(149));
    runtime.locked();
    assert!(runtime.activity.is_empty());
}
