//! What the webview sends for logins, read as the core reads it.

use super::*;

#[test]
fn a_secret_change_names_what_it_does() {
    let parse = |text: &str| serde_json::from_str::<SecretChange>(text);
    assert_eq!(parse(r#"{"type":"keep"}"#).unwrap(), SecretChange::Keep);
    assert_eq!(parse(r#"{"type":"set","value":"x"}"#).unwrap(), SecretChange::Set { value: Zeroizing::new("x".into()) });
    assert_eq!(parse(r#"{"type":"clear"}"#).unwrap(), SecretChange::Clear);
    assert_eq!(parse(r#"{"type":"generated"}"#).unwrap(), SecretChange::Generated);
    assert!(parse(r#"{"type":"set"}"#).is_err(), "a value to set");
    assert!(parse(r#"{"type":"set","value":"x","path":"/etc/passwd"}"#).is_err(), "nothing else");
    // A change without a value ignores what else comes with it, as a command without fields does
    // (serde cannot refuse it there): nothing it does depends on it.
    assert_eq!(parse(r#"{"type":"keep","value":"x"}"#).unwrap(), SecretChange::Keep);
}

#[test]
fn a_patch_tells_an_unlinked_account_from_one_left_alone() {
    let unlink: ItemPatch = serde_json::from_str(r#"{"totp_entry":null}"#).unwrap();
    assert_eq!(unlink.totp_entry, Some(None));
    let untouched: ItemPatch = serde_json::from_str("{}").unwrap();
    assert_eq!(untouched.totp_entry, None);
    let id = Uuid::from_u128(5);
    let link: ItemPatch = serde_json::from_str(&format!(r#"{{"totp_entry":"{id}"}}"#)).unwrap();
    assert_eq!(link.totp_entry, Some(Some(id)));
    assert!(serde_json::from_str::<ItemPatch>(r#"{"path":"/etc/passwd"}"#).is_err(), "no field but its own");
    assert!(serde_json::from_str::<ItemDraft>(r#"{"title":"x","secret":"y"}"#).is_err());
}

#[test]
fn urls_tags_and_domains_are_kept_clean_and_bounded() {
    let urls = clean_urls(vec![
        UrlDraft { url: "  https://example.com/  ".into(), matching: Matching::Host },
        UrlDraft { url: " ".into(), matching: Matching::Domain },
    ])
    .unwrap();
    assert_eq!(urls.iter().map(|u| (u.url.as_str(), u.rule())).collect::<Vec<_>>(), [("https://example.com/", Matching::Host)]);
    assert!(clean_urls(vec![UrlDraft { url: "https://a\u{7}.example/".into(), matching: Matching::Domain }]).is_err());
    assert!(clean_urls(vec![UrlDraft { url: "x".repeat(MAX_URL_CHARS + 1), matching: Matching::Domain }]).is_err());
    assert_eq!(clean_tags(vec!["Work".into(), "work".into(), "  Home ".into(), String::new()]).unwrap(), ["Work", "Home"]);
    assert!(clean_tags((0..=MAX_TAGS).map(|n| format!("t{n}")).collect()).is_err());
    assert!(clean_tags(vec!["t".repeat(MAX_TAG_CHARS + 1)]).is_err());
    let domains = clean_domains(&["https://User@Login.Example.com:8443/path?q".into(), "login.example.com.".into(), "[::1]:80".into()]).unwrap();
    assert_eq!(domains, ["login.example.com", "[::1]"]);
    assert!(clean_domains(&[]).is_err());
    assert!(clean_domains(&["a b.example".into()]).is_err());
    assert!(clean_domains(&(0..=MAX_RULE_DOMAINS).map(|n| format!("d{n}.example")).collect::<Vec<_>>()).is_err());
    assert_eq!(rule_id(SiteRuleKind::NeverSave, &["example.com".into()]), SiteRule::single_id(SiteRuleKind::NeverSave, "example.com"));
    assert_ne!(
        rule_id(SiteRuleKind::Equivalent, &["a.example".into(), "b.example".into()]),
        rule_id(SiteRuleKind::Equivalent, &["a.example".into(), "b.example".into()])
    );
    assert!(checked_value(Zeroizing::new("v".repeat(MAX_VALUE_BYTES + 1))).is_err());
    assert_eq!(&*checked_value(Zeroizing::new(" padded ".into())).unwrap(), " padded ", "a value is kept as typed");
    assert!(clean_text("  x\u{7}y  ", 10).is_ok_and(|t| t == "xy"));
    assert!(clean_text("toolong", 3).is_err());
}

#[test]
fn a_login_holding_a_later_versions_values_is_read_only() {
    let mut item = VaultItem::login(Uuid::from_u128(1), 1, lockra_sync::Hlc::at(1), 1);
    assert!(!holds_later_values(&item));
    let mut later = item.clone();
    later.extra.insert("expires".into(), serde_json::json!("2030"));
    assert!(holds_later_values(&later));
    let mut later = item.clone();
    later.color = Lenient::Unknown("lime".into());
    assert!(holds_later_values(&later));
    let mut later = item.clone();
    later.autofill.fill_otp = Lenient::Unknown("ask".into());
    assert!(holds_later_values(&later));
    item.write_field(Uuid::from_u128(2), Purpose::Custom, lockra_sync::Hlc::at(2), 2, 2, |f| f.purpose = Lenient::Unknown("card_number".into()));
    assert!(holds_later_values(&item));
    let view = ItemView::of(&item);
    assert!(view.read_only && view.fields.is_empty(), "a field of a later purpose is no custom field here");
}

#[test]
fn the_history_names_its_values_by_their_write() {
    let mut item = VaultItem::login(Uuid::from_u128(1), 1, lockra_sync::Hlc::at(1), 1);
    let field = Uuid::from_u128(2);
    for (n, value) in ["first", "second"].into_iter().enumerate() {
        let n = n as u64 + 2;
        item.write_field(field, Purpose::Password, lockra_sync::Hlc::at(n), n * 10, n, |f| {
            f.value = Zeroizing::new(value.into());
            f.hidden = true;
        });
    }
    let views = past_views(&item);
    assert_eq!(views.len(), 1);
    assert_eq!((views[0].id.as_str(), views[0].value.as_str(), views[0].conflict), ("20", "first", false));
    assert!(past_value(&item, field, "20").is_some());
    assert!(past_value(&item, field, "21").is_none() && past_value(&item, field, "not a number").is_none());
    assert!(past_value(&item, Uuid::from_u128(9), "20").is_none());
}
