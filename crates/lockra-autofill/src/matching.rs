//! Which saved URLs match a page, and how closely: the six rules a saved URL can have, the four
//! levels a match reaches, and the selection of the items at the best level.

use serde::{Deserialize, Serialize};

use crate::address::WebUrl;
use crate::equivalent::EquivalentDomains;

/// How a saved URL is compared with the page. The saved URL is normalized as the page's is
/// ([`WebUrl::parse_saved`]: text without a scheme reads as `https://`); one that is no `http` or
/// `https` URL never matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Matching {
    /// The same registrable domain (`login.example.com` for `example.com`), or a domain the
    /// equivalent-domain lists share it with. The default.
    #[default]
    Domain,
    /// The same host and port, the scheme's default port being none: `https://example.com` and
    /// `http://example.com` are one host, `https://example.com:8443` another.
    Host,
    /// The page's URL begins with the saved one, character for character: `https://example.com/login`
    /// covers `https://example.com/login?next=%2F` and `https://example.com/login2`.
    StartsWith,
    /// The saved text is a regular expression, searched for in the page's normalized URL without
    /// regard to case (as Bitwarden does). A blank pattern, one that does not compile, or one
    /// that would compile too large never matches.
    Regex,
    /// The page's URL is the saved one, query and fragment included.
    Exact,
    /// This saved URL never matches; the item's other URLs still do.
    Never,
}

/// How closely a saved URL matches the page, from the loosest to the closest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchLevel {
    /// [`Matching::Domain`], equivalent domains included.
    Domain,
    /// [`Matching::Host`].
    Host,
    /// [`Matching::StartsWith`] or [`Matching::Regex`].
    Pattern,
    /// [`Matching::Exact`].
    Exact,
}

/// One URL saved with an item and its rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedUrl<'a> {
    /// The URL as saved, or the pattern for [`Matching::Regex`].
    pub url: &'a str,
    /// How it is compared.
    pub matching: Matching,
}

/// A saved URL that matches the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UrlMatch {
    /// How closely.
    pub level: MatchLevel,
    /// The page is `http` while the saved URL is `https`: fill only after the user agrees.
    pub downgrade: bool,
}

/// An item at the best level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemMatch<K> {
    /// The caller's key for the item.
    pub item: K,
    /// The index of the item's saved URL that matched (the first at the best level, one without
    /// a downgrade before one with).
    pub url: usize,
    /// That URL's [`UrlMatch::downgrade`].
    pub downgrade: bool,
}

/// The items that reach the best level any item reaches, in the order they came in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BestMatches<K> {
    /// The level they share.
    pub level: MatchLevel,
    /// The items at that level; items that only reach a looser one are left out.
    pub items: Vec<ItemMatch<K>>,
}

/// The most a [`Matching::Regex`] pattern may compile to, and the most memory its search may
/// cache. Ample for URL patterns (`^https://([a-z0-9-]+\.)*example\.com/` takes a few KiB);
/// past that a pattern never matches.
const REGEX_SIZE_LIMIT: usize = 1 << 20;

/// How `saved` matches `page`, or `None`. `ports`: the port counts at the domain level too (the
/// user's choice, Settings › Browser), so a saved URL of another port, or of none where the page
/// has one, is no match; the scheme's default port is none, as for [`Matching::Host`].
pub fn match_url(page: &WebUrl, saved: SavedUrl<'_>, equivalents: &EquivalentDomains, ports: bool) -> Option<UrlMatch> {
    match saved.matching {
        Matching::Never => return None,
        // A blank pattern would match every page; it is an empty field, not a wish.
        Matching::Regex if saved.url.trim().is_empty() => return None,
        Matching::Regex => {
            let regex =
                regex::RegexBuilder::new(saved.url).case_insensitive(true).size_limit(REGEX_SIZE_LIMIT).dfa_size_limit(REGEX_SIZE_LIMIT).build().ok()?;
            // A pattern names no scheme of its own, so there is nothing to downgrade from.
            return regex.is_match(page.as_str()).then_some(UrlMatch { level: MatchLevel::Pattern, downgrade: false });
        }
        _ => {}
    }
    let url = WebUrl::parse_saved(saved.url)?;
    let level = match saved.matching {
        Matching::Domain if (url.same_site(page) || equivalents.allows(&url, page)) && (!ports || url.port() == page.port()) => MatchLevel::Domain,
        Matching::Host if url.host() == page.host() && url.port() == page.port() => MatchLevel::Host,
        Matching::StartsWith if page.as_str().starts_with(url.as_str()) => MatchLevel::Pattern,
        Matching::Exact if url.as_str() == page.as_str() => MatchLevel::Exact,
        _ => return None,
    };
    Some(UrlMatch { level, downgrade: url.is_https() && !page.is_https() })
}

/// The items whose saved URLs reach the best level any of them reaches on `page`, each with the
/// URL that got it there; `None` when no saved URL matches. `items` pairs the caller's key with
/// the item's saved URLs; `ports` as for [`match_url`].
pub fn best_matches<'a, K, U>(page: &WebUrl, items: impl IntoIterator<Item = (K, U)>, equivalents: &EquivalentDomains, ports: bool) -> Option<BestMatches<K>>
where
    U: IntoIterator<Item = SavedUrl<'a>>,
{
    let mut best: Option<BestMatches<K>> = None;
    for (item, urls) in items {
        // The item's closest URL; at the same level one without a downgrade, then the first.
        let mut closest: Option<(usize, UrlMatch)> = None;
        for (index, saved) in urls.into_iter().enumerate() {
            let Some(found) = match_url(page, saved, equivalents, ports) else { continue };
            let closer = closest.is_none_or(|(_, held)| (found.level, !found.downgrade) > (held.level, !held.downgrade));
            if closer {
                closest = Some((index, found));
            }
        }
        let Some((url, found)) = closest else { continue };
        let matched = ItemMatch { item, url, downgrade: found.downgrade };
        match &mut best {
            Some(best) if best.level == found.level => best.items.push(matched),
            Some(best) if best.level > found.level => {}
            _ => best = Some(BestMatches { level: found.level, items: vec![matched] }),
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(text: &str) -> WebUrl {
        WebUrl::parse(text).unwrap()
    }

    fn level(page_url: &str, saved: &str, matching: Matching) -> Option<MatchLevel> {
        found(page_url, saved, matching).map(|found| found.level)
    }

    fn found(page_url: &str, saved: &str, matching: Matching) -> Option<UrlMatch> {
        match_url(&page(page_url), SavedUrl { url: saved, matching }, &EquivalentDomains::default(), false)
    }

    #[test]
    fn rules_have_stable_names() {
        assert_eq!(Matching::default(), Matching::Domain);
        let names =
            serde_json::to_string(&[Matching::Domain, Matching::Host, Matching::StartsWith, Matching::Regex, Matching::Exact, Matching::Never]).unwrap();
        assert_eq!(names, r#"["domain","host","starts_with","regex","exact","never"]"#);
        let levels = serde_json::to_string(&[MatchLevel::Domain, MatchLevel::Host, MatchLevel::Pattern, MatchLevel::Exact]).unwrap();
        assert_eq!(levels, r#"["domain","host","pattern","exact"]"#);
        assert!(MatchLevel::Exact > MatchLevel::Pattern && MatchLevel::Pattern > MatchLevel::Host && MatchLevel::Host > MatchLevel::Domain);
    }

    #[test]
    fn domain_rule_matches_the_registrable_domain() {
        use Matching::Domain;
        assert_eq!(level("https://login.example.com/a", "example.com", Domain), Some(MatchLevel::Domain));
        assert_eq!(level("https://example.com:8443/", "https://www.example.com/login", Domain), Some(MatchLevel::Domain));
        assert_eq!(level("https://example.net/", "example.com", Domain), None);
        assert_eq!(level("https://example.com.evil.example/", "example.com", Domain), None);
        assert_eq!(level("https://notexample.com/", "example.com", Domain), None);
        // Public suffix boundaries.
        assert_eq!(level("https://b.example.co.uk/", "https://a.example.co.uk/", Domain), Some(MatchLevel::Domain));
        assert_eq!(level("https://bar.github.io/", "https://foo.github.io/", Domain), None);
        assert_eq!(level("https://foo.github.io/", "https://github.io/", Domain), None);
        assert_eq!(level("https://example.co.uk/", "co.uk", Domain), None);
        assert_eq!(level("https://co.uk/", "co.uk", Domain), Some(MatchLevel::Domain));
        // One host, however it is written.
        assert_eq!(level("https://xn--r8jz45g.jp/", "https://www.例え.jp/", Domain), Some(MatchLevel::Domain));
        assert_eq!(level("https://EXAMPLE.com./", "Example.COM", Domain), Some(MatchLevel::Domain));
    }

    #[test]
    fn addresses_and_single_labels_match_only_themselves() {
        use Matching::Domain;
        assert_eq!(level("http://192.168.1.1:8080/", "https://192.168.1.1/", Domain), Some(MatchLevel::Domain));
        assert_eq!(level("http://10.0.1.1/", "http://192.168.1.1/", Domain), None);
        assert_eq!(level("http://192.168.1.1/", "http://1.1/", Domain), None);
        assert_eq!(level("http://[::1]:3000/", "http://[::1]/", Domain), Some(MatchLevel::Domain));
        assert_eq!(level("http://[::2]/", "http://[::1]/", Domain), None);
        assert_eq!(level("http://localhost:3000/", "http://localhost:8080/", Domain), Some(MatchLevel::Domain));
        assert_eq!(level("http://intranet/", "http://localhost/", Domain), None);
        assert_eq!(level("http://app.localhost/", "http://localhost/", Domain), None);
    }

    #[test]
    fn with_ports_the_domain_rule_wants_the_page_s_port_too() {
        let ports = |page_url: &str, saved: &str, matching: Matching| {
            match_url(&page(page_url), SavedUrl { url: saved, matching }, &EquivalentDomains::default(), true).map(|found| found.level)
        };
        use Matching::{Domain, Exact, Host, Regex, StartsWith};
        // One address, a login per port.
        assert_eq!(ports("http://192.168.1.10:5000/", "http://192.168.1.10:5000/", Domain), Some(MatchLevel::Domain));
        assert_eq!(ports("http://192.168.1.10:8080/", "http://192.168.1.10:5000/", Domain), None);
        assert_eq!(ports("https://nas.example.com:5001/", "example.com:5001", Domain), Some(MatchLevel::Domain));
        // A URL saved without a port is the scheme's default port, and so is a page without one.
        assert_eq!(ports("https://example.com:8443/", "example.com", Domain), None);
        assert_eq!(ports("https://login.example.com/", "example.com", Domain), Some(MatchLevel::Domain));
        assert_eq!(ports("http://example.com/", "https://example.com:443/", Domain), Some(MatchLevel::Domain));
        assert_eq!(ports("https://example.com/", "https://example.com:8443/", Domain), None);
        // Equivalent domains too.
        let user = EquivalentDomains::new([["example.com", "example.net"]]);
        let saved = SavedUrl { url: "https://example.com:8443/", matching: Domain };
        assert_eq!(match_url(&page("https://example.net:8443/"), saved, &user, true).map(|found| found.level), Some(MatchLevel::Domain));
        assert_eq!(match_url(&page("https://example.net/"), saved, &user, true), None);
        // The other rules are as they were: the host rule and the URLs' text already have the port.
        assert_eq!(ports("https://example.com:8443/", "example.com:8443", Host), Some(MatchLevel::Host));
        assert_eq!(ports("https://example.com:8443/", "example.com", Host), None);
        assert_eq!(ports("https://example.com:8443/a", "https://example.com:8443/", StartsWith), Some(MatchLevel::Pattern));
        assert_eq!(ports("https://example.com:8443/", "https://example.com:8443/", Exact), Some(MatchLevel::Exact));
        assert_eq!(ports("https://example.com:8443/", r"example\.com", Regex), Some(MatchLevel::Pattern));
        // Off, the port does not count at the domain level (the default).
        assert_eq!(level("http://192.168.1.10:8080/", "http://192.168.1.10:5000/", Domain), Some(MatchLevel::Domain));
        // And the best level is chosen among what is left.
        let items = vec![
            ("nas", urls(&[("http://192.168.1.10:5000/", Domain)])),
            ("router", urls(&[("http://192.168.1.10/", Domain)])),
            ("both", urls(&[("http://192.168.1.10:8080/", Domain), ("http://192.168.1.10:5000/", Domain)])),
        ];
        let best = best_matches(&page("http://192.168.1.10:5000/login"), items.clone(), &EquivalentDomains::default(), true).unwrap();
        assert_eq!(best.items, [ItemMatch { item: "nas", url: 0, downgrade: false }, ItemMatch { item: "both", url: 1, downgrade: false }]);
        let best = best_matches(&page("http://192.168.1.10/"), items, &EquivalentDomains::default(), true).unwrap();
        assert_eq!(best.items, [ItemMatch { item: "router", url: 0, downgrade: false }]);
    }

    #[test]
    fn host_rule_matches_host_and_port() {
        use Matching::Host;
        assert_eq!(level("https://example.com/login", "example.com", Host), Some(MatchLevel::Host));
        assert_eq!(level("http://example.com/", "https://example.com/", Host), Some(MatchLevel::Host));
        assert_eq!(level("http://example.com:80/", "https://example.com:443/", Host), Some(MatchLevel::Host));
        assert_eq!(level("https://www.example.com/", "example.com", Host), None);
        assert_eq!(level("https://example.com:8443/", "example.com", Host), None);
        assert_eq!(level("https://example.com:8443/", "example.com:8443", Host), Some(MatchLevel::Host));
        assert_eq!(level("http://example.com:443/", "https://example.com/", Host), None);
        assert_eq!(level("http://192.168.1.1:8080/", "192.168.1.1:8080", Host), Some(MatchLevel::Host));
        assert_eq!(level("http://192.168.1.1:8081/", "192.168.1.1:8080", Host), None);
        assert_eq!(level("http://[::1]:3000/x", "http://[::1]:3000", Host), Some(MatchLevel::Host));
    }

    #[test]
    fn starts_with_compares_normalized_urls() {
        use Matching::StartsWith;
        assert_eq!(level("https://example.com/login?next=%2F", "https://example.com/login", StartsWith), Some(MatchLevel::Pattern));
        assert_eq!(level("https://example.com/loginx", "https://example.com/login", StartsWith), Some(MatchLevel::Pattern));
        assert_eq!(level("https://example.com/other", "https://example.com/login", StartsWith), None);
        // The saved host ends with the slash the parser adds, so another host cannot extend it.
        assert_eq!(level("https://example.com.evil.example/", "https://example.com", StartsWith), None);
        assert_eq!(level("https://example.com/", "example.com", StartsWith), Some(MatchLevel::Pattern));
        assert_eq!(level("https://example.com@evil.example/", "https://example.com", StartsWith), None);
        assert_eq!(level("https://user:pw@example.com/a", "https://example.com/a", StartsWith), Some(MatchLevel::Pattern));
        assert_eq!(level("https://EXAMPLE.com/Login", "https://example.COM/Login", StartsWith), Some(MatchLevel::Pattern));
        // The path keeps its case.
        assert_eq!(level("https://example.com/login", "https://example.com/Login", StartsWith), None);
        // A scheme is part of the text.
        assert_eq!(level("http://example.com/login", "https://example.com/login", StartsWith), None);
        assert_eq!(level("https://app.example.com/#/login/sso", "https://app.example.com/#/login", StartsWith), Some(MatchLevel::Pattern));
    }

    #[test]
    fn exact_compares_whole_normalized_urls() {
        use Matching::Exact;
        assert_eq!(level("https://example.com/login", "https://example.com/login", Exact), Some(MatchLevel::Exact));
        assert_eq!(level("https://example.com/", "example.com", Exact), Some(MatchLevel::Exact));
        assert_eq!(level("https://example.com:443/login", "HTTPS://EXAMPLE.COM/login", Exact), Some(MatchLevel::Exact));
        assert_eq!(level("https://example.com/login?a=1", "https://example.com/login", Exact), None);
        assert_eq!(level("https://example.com/#/login", "https://example.com/#/logout", Exact), None);
        assert_eq!(level("http://example.com/login", "https://example.com/login", Exact), None);
    }

    #[test]
    fn regex_searches_the_normalized_url_without_case() {
        use Matching::Regex;
        assert_eq!(level("https://a.example.com/login", r"^https://([a-z0-9-]+\.)*example\.com/", Regex), Some(MatchLevel::Pattern));
        assert_eq!(level("https://A.Example.com/LOGIN", r"example\.com/login", Regex), Some(MatchLevel::Pattern));
        assert_eq!(level("https://example.net/", r"example\.com", Regex), None);
        // No user name for a pattern to find: the host is what follows the @.
        assert_eq!(level("https://example.com@evil.example/", r"example\.com", Regex), None);
        // The pattern is not a URL: no scheme is added, no host compared.
        assert_eq!(level("http://example.com/", r"^http://example\.com/$", Regex), Some(MatchLevel::Pattern));
        assert_eq!(found("http://example.com/", r"^http", Regex).map(|found| found.downgrade), Some(false));
        // Invalid, empty of meaning or too large: never a match.
        assert_eq!(level("https://example.com/", r"(unclosed", Regex), None);
        assert_eq!(level("https://example.com/", r"\w{100}|example", Regex), None);
        assert_eq!(level("https://example.com/", r"[\p{L}\p{N}]{60}|example", Regex), None);
        // A blank pattern (an import's empty URL) would match every page: it is no pattern.
        assert_eq!(level("https://example.com/", "", Regex), None);
        assert_eq!(level("https://example.com/", "  ", Regex), None);
        assert_eq!(level("https://example.com/", ".*", Regex), Some(MatchLevel::Pattern), "every page, as the user asked");
    }

    #[test]
    fn never_matches_nothing() {
        assert_eq!(level("https://example.com/", "https://example.com/", Matching::Never), None);
        assert_eq!(level("https://example.com/", ".*", Matching::Never), None);
    }

    #[test]
    fn saved_urls_of_other_schemes_never_match() {
        for matching in [Matching::Domain, Matching::Host, Matching::StartsWith, Matching::Exact] {
            assert_eq!(level("https://example.com/", "ftp://example.com/", matching), None);
            assert_eq!(level("https://example.com/", "androidapp://com.example", matching), None);
            assert_eq!(level("https://example.com/", "not a url", matching), None);
            assert_eq!(level("https://example.com/", "", matching), None);
        }
    }

    #[test]
    fn http_pages_for_https_urls_are_a_downgrade() {
        let downgrade = |page_url: &str, saved: &str, matching: Matching| found(page_url, saved, matching).map(|found| found.downgrade);
        assert_eq!(downgrade("http://example.com/", "https://example.com/", Matching::Domain), Some(true));
        assert_eq!(downgrade("http://login.example.com/", "example.com", Matching::Domain), Some(true));
        assert_eq!(downgrade("http://example.com/", "example.com", Matching::Host), Some(true));
        assert_eq!(downgrade("http://example.com/", "http://example.com/", Matching::Host), Some(false));
        assert_eq!(downgrade("https://example.com/", "http://example.com/", Matching::Domain), Some(false));
        assert_eq!(downgrade("https://example.com/", "https://example.com/", Matching::Domain), Some(false));
    }

    #[test]
    fn equivalent_domains_count_as_the_domain_level() {
        let user = EquivalentDomains::new([["example.com", "example.net"]]);
        let check = |page_url: &str, saved: &str, matching: Matching, equivalents: &EquivalentDomains| {
            match_url(&page(page_url), SavedUrl { url: saved, matching }, equivalents, false)
        };
        assert_eq!(check("https://www.airbnb.co.uk/", "airbnb.com", Matching::Domain, &user), Some(UrlMatch { level: MatchLevel::Domain, downgrade: false }));
        assert_eq!(
            check("http://login.example.net/", "https://example.com/", Matching::Domain, &user),
            Some(UrlMatch { level: MatchLevel::Domain, downgrade: true })
        );
        // Only the domain rule widens; the stricter rules mean what they say.
        assert_eq!(check("https://example.net/", "https://example.com/", Matching::Host, &user), None);
        assert_eq!(check("https://example.net/", "https://example.com/", Matching::StartsWith, &user), None);
        assert_eq!(check("https://example.net/", "https://example.com/", Matching::Domain, &EquivalentDomains::default()), None);
    }

    fn urls(list: &[(&'static str, Matching)]) -> Vec<SavedUrl<'static>> {
        list.iter().map(|&(url, matching)| SavedUrl { url, matching }).collect()
    }

    #[test]
    fn only_the_best_level_is_offered() {
        let items = vec![
            ("domain", urls(&[("example.com", Matching::Domain)])),
            ("host", urls(&[("https://login.example.com", Matching::Host)])),
            ("other", urls(&[("example.net", Matching::Domain)])),
            ("host too", urls(&[("example.net", Matching::Domain), ("login.example.com", Matching::Host)])),
            ("never", urls(&[("https://login.example.com/", Matching::Never)])),
        ];
        let best = best_matches(&page("https://login.example.com/signin"), items, &EquivalentDomains::default(), false).unwrap();
        assert_eq!(best.level, MatchLevel::Host);
        assert_eq!(best.items, [ItemMatch { item: "host", url: 0, downgrade: false }, ItemMatch { item: "host too", url: 1, downgrade: false }]);

        let exact = vec![
            ("pattern", urls(&[(r"login\.example\.com", Matching::Regex)])),
            ("exact", urls(&[("https://login.example.com/signin", Matching::Exact)])),
            ("starts", urls(&[("https://login.example.com/sign", Matching::StartsWith)])),
        ];
        let best = best_matches(&page("https://login.example.com/signin"), exact, &EquivalentDomains::default(), false).unwrap();
        assert_eq!(best.level, MatchLevel::Exact);
        assert_eq!(best.items, [ItemMatch { item: "exact", url: 0, downgrade: false }]);

        let patterns = vec![
            ("regex", urls(&[(r"example", Matching::Regex)])),
            ("starts", urls(&[("https://login.example.com/", Matching::StartsWith)])),
            ("domain", urls(&[("example.com", Matching::Domain)])),
        ];
        let best = best_matches(&page("https://login.example.com/signin"), patterns, &EquivalentDomains::default(), false).unwrap();
        assert_eq!(best.level, MatchLevel::Pattern);
        assert_eq!(best.items.iter().map(|found| found.item).collect::<Vec<_>>(), ["regex", "starts"]);
    }

    #[test]
    fn an_item_prefers_its_url_without_a_downgrade() {
        let items = vec![(
            7,
            urls(&[("https://example.com", Matching::Domain), ("http://example.com", Matching::Domain), ("https://example.com/b", Matching::Domain)]),
        )];
        let best = best_matches(&page("http://www.example.com/"), items, &EquivalentDomains::default(), false).unwrap();
        assert_eq!(best.items, [ItemMatch { item: 7, url: 1, downgrade: false }]);
        let only_https = vec![(7, urls(&[("https://example.com", Matching::Domain), ("https://example.com/b", Matching::Domain)]))];
        let best = best_matches(&page("http://www.example.com/"), only_https, &EquivalentDomains::default(), false).unwrap();
        assert_eq!(best.items, [ItemMatch { item: 7, url: 0, downgrade: true }]);
    }

    #[test]
    fn nothing_matches_nothing() {
        let items = vec![("a", urls(&[("example.net", Matching::Domain)])), ("b", urls(&[])), ("c", urls(&[("example.com", Matching::Never)]))];
        assert_eq!(best_matches(&page("https://example.com/"), items, &EquivalentDomains::default(), false), None);
        assert_eq!(best_matches::<u8, Vec<SavedUrl<'_>>>(&page("https://example.com/"), [], &EquivalentDomains::default(), false), None);
    }
}
