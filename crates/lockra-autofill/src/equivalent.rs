//! Domains that share one sign-in: Apple's shared-credentials list, bundled, and groups the user
//! adds. A credential saved for one domain of a group may be offered on another, at the level of
//! a registrable-domain match.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::address::WebUrl;
use crate::data;

/// One entry of `shared-credentials.json`, as `quirks/schemas/shared-credentials-schema.json` at
/// the pinned commit defines it: a group shared both ways, or credentials of the `from` domains
/// offered on the `to` domains. Unknown keys fail the list (and the tests) rather than vanish.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Entry {
    Shared(SharedEntry),
    OneWay(OneWayEntry),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SharedEntry {
    shared: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OneWayEntry {
    from: Vec<String>,
    to: Vec<String>,
    /// Logins are no longer served on the `from` domains. Matching is the same either way: their
    /// credentials are still offered on the `to` domains, and never the other way round.
    #[serde(default, rename = "fromDomainsAreObsoleted")]
    #[allow(dead_code)]
    obsoleted: bool,
}

fn bundled_entries(json: &str) -> Result<Vec<Entry>, serde_json::Error> {
    serde_json::from_str(json)
}

/// The bundled list, indexed on first use. The file is pinned and parsed in the tests, so the
/// empty fallback is never what a build ships with.
static BUNDLED: LazyLock<DomainIndex> = LazyLock::new(|| {
    let mut index = DomainIndex::default();
    for (group, entry) in bundled_entries(data::SHARED_CREDENTIALS).unwrap_or_default().iter().enumerate() {
        match entry {
            Entry::Shared(shared) => index.insert_all(group, Role::Shared, &shared.shared),
            Entry::OneWay(one_way) => {
                index.insert_all(group, Role::From, &one_way.from);
                index.insert_all(group, Role::To, &one_way.to);
            }
        }
    }
    index
});

/// Where a domain stands in a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Shares with every other `Shared` domain of the group.
    Shared,
    /// Its credentials are offered on the group's `To` domains.
    From,
    /// Offered the credentials of the group's `From` domains.
    To,
}

/// Normalized domain to the groups it belongs to.
#[derive(Debug, Clone, Default)]
struct DomainIndex {
    domains: HashMap<String, Vec<(usize, Role)>>,
}

impl DomainIndex {
    /// Add each domain (or URL) of `list`; addresses and text that is no web host are skipped.
    fn insert_all<D: AsRef<str>>(&mut self, group: usize, role: Role, list: impl IntoIterator<Item = D>) {
        for text in list {
            if let Some(url) = WebUrl::parse_saved(text.as_ref()).filter(|url| !url.is_ip()) {
                self.domains.entry(url.host().to_owned()).or_default().push((group, role));
            }
        }
    }

    /// The groups `url` belongs to: those of its host and of each parent domain down to its site.
    fn memberships<'a>(&'a self, url: &'a WebUrl) -> impl Iterator<Item = (usize, Role)> + 'a {
        url.domains().filter_map(|domain| self.domains.get(domain)).flatten().copied()
    }

    fn allows(&self, saved: &WebUrl, page: &WebUrl) -> bool {
        if self.domains.is_empty() || saved.is_ip() || page.is_ip() {
            return false;
        }
        self.memberships(page).any(|(group, page_role)| {
            self.memberships(saved).any(|(saved_group, saved_role)| {
                saved_group == group && matches!((saved_role, page_role), (Role::Shared, Role::Shared) | (Role::From, Role::To))
            })
        })
    }
}

/// The domains whose credentials are offered on each other: the bundled list plus the user's
/// groups. A domain in a group covers its subdomains too, but never a parent: `dnt.abine.com`
/// does not bring in `www.abine.com`, and a group holding `co.uk` does not reach `shop.co.uk`.
/// Addresses take no part. Building one is cheap; the bundled list is indexed once per process.
#[derive(Debug, Clone, Default)]
pub struct EquivalentDomains {
    user: DomainIndex,
}

impl EquivalentDomains {
    /// The bundled list and the user's `groups`, each a set of domains (or URLs, whose host
    /// counts) that share their sign-in both ways.
    pub fn new<G, D>(groups: impl IntoIterator<Item = G>) -> Self
    where
        G: IntoIterator<Item = D>,
        D: AsRef<str>,
    {
        let mut user = DomainIndex::default();
        for (group, domains) in groups.into_iter().enumerate() {
            user.insert_all(group, Role::Shared, domains);
        }
        Self { user }
    }

    /// Whether a credential saved for `saved` may be offered on `page`.
    pub fn allows(&self, saved: &WebUrl, page: &WebUrl) -> bool {
        BUNDLED.allows(saved, page) || self.user.allows(saved, page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> WebUrl {
        WebUrl::parse_saved(text).unwrap()
    }

    fn bundled(saved: &str, page: &str) -> bool {
        EquivalentDomains::default().allows(&url(saved), &url(page))
    }

    #[test]
    fn shared_groups_work_both_ways() {
        // {"shared": ["airbnb.com.ar", …, "airbnb.co.uk", …, "airbnb.com", …]}
        assert!(bundled("https://www.airbnb.com/login", "https://www.airbnb.co.uk/"));
        assert!(bundled("airbnb.co.uk", "airbnb.com"));
        assert!(bundled("airbnb.com.br", "https://es.airbnb.com.ar/"));
        assert!(!bundled("airbnb.com", "airbnb.example"));
    }

    #[test]
    fn from_to_entries_work_one_way() {
        // {"from": ["adobelogin.com", "behance.net"], "to": ["adobe.com"]}
        assert!(bundled("https://adobelogin.com/", "https://account.adobe.com/"));
        assert!(bundled("behance.net", "adobe.com"));
        assert!(!bundled("adobe.com", "adobelogin.com"));
        // Domains on the same side are not shared among themselves.
        assert!(!bundled("adobelogin.com", "behance.net"));
        // {"from": ["discordapp.com"], "to": ["discord.com"], "fromDomainsAreObsoleted": true}
        assert!(bundled("discordapp.com", "discord.com"));
        assert!(!bundled("discord.com", "discordapp.com"));
        // {"from": ["prog-8.com"], "to": ["progate.com"], "fromDomainsAreObsoleted": true}: a shared
        // group until upstream's 91dff9a.
        assert!(bundled("prog-8.com", "progate.com"));
        assert!(!bundled("progate.com", "prog-8.com"));
    }

    #[test]
    fn a_listed_subdomain_covers_only_itself_and_below() {
        // {"shared": ["dnt.abine.com", "ironvest.com"]}
        assert!(bundled("https://dnt.abine.com/", "https://ironvest.com/"));
        assert!(bundled("https://login.ironvest.com/", "https://x.dnt.abine.com/"));
        assert!(!bundled("https://www.abine.com/", "https://ironvest.com/"));
        assert!(!bundled("https://ironvest.com/", "https://abine.com/"));
        // {"from": ["costco.ca"], "to": ["signin.costco.com"]}
        assert!(bundled("costco.ca", "https://signin.costco.com/"));
        assert!(!bundled("costco.ca", "https://www.costco.com/"));
    }

    #[test]
    fn user_groups_add_to_the_bundled_list() {
        let domains = EquivalentDomains::new([vec!["example.com", "https://login.example.net/path"], vec!["Example.ORG", "例え.jp"]]);
        assert!(domains.allows(&url("https://www.example.com/"), &url("https://login.example.net/")));
        assert!(domains.allows(&url("https://login.example.net/"), &url("https://example.com/")));
        // The host of a URL counts, with what lies below it, not its parent.
        assert!(!domains.allows(&url("https://example.com/"), &url("https://www.example.net/")));
        assert!(domains.allows(&url("https://example.org/"), &url("https://xn--r8jz45g.jp/")));
        assert!(!domains.allows(&url("https://example.com/"), &url("https://example.org/")));
        // The bundled list still applies.
        assert!(domains.allows(&url("airbnb.com"), &url("airbnb.co.uk")));
    }

    #[test]
    fn user_groups_ignore_what_is_not_a_domain() {
        let domains = EquivalentDomains::new([vec!["192.168.1.1", "example.com", "", "mailto:me@example.net", "nas.local"]]);
        assert!(!domains.allows(&url("http://192.168.1.1/"), &url("https://example.com/")));
        assert!(!domains.allows(&url("https://example.com/"), &url("http://192.168.1.1/")));
        assert!(!domains.allows(&url("https://example.com/"), &url("https://example.net/")));
        assert!(domains.allows(&url("https://example.com/"), &url("http://nas.local/")));
    }

    #[test]
    fn a_public_suffix_in_a_group_covers_only_itself() {
        // Grouping "co.uk" with a domain must not reach every site under co.uk.
        let domains = EquivalentDomains::new([["co.uk", "example.com"]]);
        assert!(!domains.allows(&url("https://example.com/"), &url("https://shop.co.uk/")));
        assert!(domains.allows(&url("https://example.com/"), &url("https://co.uk/")));
    }

    #[test]
    fn the_bundled_list_follows_its_schema() {
        let entries = bundled_entries(crate::data::SHARED_CREDENTIALS).unwrap();
        // 112 entries at the pinned commit: 55 shared groups and 57 one-way ones.
        assert_eq!(entries.len(), 112);
        assert_eq!(entries.iter().filter(|entry| matches!(entry, Entry::Shared(_))).count(), 55);
        for entry in &entries {
            let lists: Vec<&Vec<String>> = match entry {
                Entry::Shared(shared) => vec![&shared.shared],
                Entry::OneWay(one_way) => vec![&one_way.from, &one_way.to],
            };
            for list in lists {
                assert!(!list.is_empty());
                for domain in list {
                    assert!(WebUrl::parse_saved(domain).is_some_and(|url| url.host() == domain), "{domain} is a normalized domain");
                }
            }
        }
        // An entry outside the schema fails the whole list, so an update cannot drop one quietly.
        assert!(bundled_entries(r#"[{"shared": ["a.example"], "from": ["b.example"]}]"#).is_err());
        assert!(bundled_entries(r#"[{"from": ["a.example"]}]"#).is_err());
        assert!(bundled_entries(r#"[{"from": ["a.example"], "to": ["b.example"], "fromDomainsAreObsoleted": true}]"#).is_ok());
    }
}
