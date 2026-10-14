//! Web addresses as autofill compares them: `http` and `https` URLs with a normalized host, and the
//! site, the registrable domain by the Public Suffix List, that a host belongs to.

use url::{Host, ParseError, Url};

/// An `http` or `https` URL with a host, normalized for comparison: the host in lowercase ASCII
/// (IDN in punycode, as the WHATWG URL parser writes it) without a trailing dot, no user name or
/// password, the scheme's default port left out. The path, query and fragment stay as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebUrl {
    url: Url,
    site: String,
    ip: bool,
}

impl WebUrl {
    /// A page's address, as the browser reports it. `None` for anything but an absolute `http`
    /// or `https` URL with a host (`about:`, `data:`, `blob:`, `file:`, extension pages): autofill
    /// never acts there.
    pub fn parse(text: &str) -> Option<Self> {
        Self::normalized(Url::parse(text).ok()?)
    }

    /// A URL saved with an item or typed by the user. As [`WebUrl::parse`], except that text
    /// without a scheme reads as `https://` followed by the text (`example.com/login`); such
    /// text must not carry a user name, so `mailto:me@example.com` is not read as a host.
    pub fn parse_saved(text: &str) -> Option<Self> {
        let text = text.trim();
        match Url::parse(text) {
            // `example.com:8080` and `localhost:3000` parse too, as a scheme and a path: only a
            // scheme the WHATWG parser knows, or one followed by `//`, is a scheme here.
            Ok(url) if url.is_special() || text.split_once(':').is_some_and(|(_, rest)| rest.starts_with("//")) => Self::normalized(url),
            Ok(_) | Err(ParseError::RelativeUrlWithoutBase) => {
                let url = Url::parse(&format!("https://{text}")).ok()?;
                if !url.username().is_empty() || url.password().is_some() {
                    return None;
                }
                Self::normalized(url)
            }
            Err(_) => None,
        }
    }

    fn normalized(mut url: Url) -> Option<Self> {
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        // Both only fail for URLs without a host, which http and https URLs always have.
        url.set_username("").ok()?;
        url.set_password(None).ok()?;
        let (site, ip) = match url.host()? {
            Host::Domain(domain) => {
                let domain = domain.strip_suffix('.').unwrap_or(domain).to_owned();
                if domain.split('.').any(str::is_empty) {
                    return None;
                }
                if url.host_str() != Some(domain.as_str()) {
                    url.set_host(Some(&domain)).ok()?;
                }
                (psl::domain_str(&domain).unwrap_or(&domain).to_owned(), false)
            }
            // The suffix list knows nothing of addresses: an address is a site of its own.
            Host::Ipv4(_) | Host::Ipv6(_) => (url.host_str()?.to_owned(), true),
        };
        Some(Self { url, site, ip })
    }

    /// The normalized URL.
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }

    /// The host: a domain in lowercase ASCII, an IPv4 address, or an IPv6 address in brackets.
    pub fn host(&self) -> &str {
        self.url.host_str().unwrap_or_default()
    }

    /// The port when it is not the scheme's default, so `http://example.com:80` and
    /// `https://example.com` both have none.
    pub fn port(&self) -> Option<u16> {
        self.url.port()
    }

    /// Whether the scheme is `https`.
    pub fn is_https(&self) -> bool {
        self.url.scheme() == "https"
    }

    /// The site: the registrable domain (`example.co.uk` for `a.example.co.uk`, `foo.github.io`
    /// for itself), or the host itself when it has none: an IP address, a single label such as
    /// `localhost`, or a public suffix such as `co.uk`.
    pub fn site(&self) -> &str {
        &self.site
    }

    /// Whether both URLs belong to the same site (the frame rule: a frame of another site is not
    /// filled). Schemes and ports do not count.
    pub fn same_site(&self, other: &WebUrl) -> bool {
        self.site == other.site
    }

    /// Whether the host is an IP address.
    pub(crate) fn is_ip(&self) -> bool {
        self.ip
    }

    /// The host, then each parent domain down to the site: `a.b.example.com`, `b.example.com`,
    /// `example.com`. A host that is its own site (an address, `localhost`, `co.uk`) yields only
    /// itself, so nothing reaches past the site to a public suffix.
    pub(crate) fn domains(&self) -> impl Iterator<Item = &str> {
        let site_len = self.site.len();
        std::iter::successors(
            Some(self.host()),
            move |current| {
                if current.len() <= site_len { None } else { current.split_once('.').map(|(_, parent)| parent) }
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(text: &str) -> WebUrl {
        WebUrl::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    #[test]
    fn only_http_and_https_pages_with_a_host() {
        for text in [
            "about:blank",
            "about:srcdoc",
            "data:text/html,<form><input type=password></form>",
            "blob:https://example.com/0b2d6f3e",
            "file:///home/me/login.html",
            "chrome-extension://abcdefghijklmnop/popup.html",
            "moz-extension://0b2d6f3e/popup.html",
            "javascript:alert(1)",
            "ftp://example.com/",
            "wss://example.com/",
            "https://",
            "example.com",
            "",
        ] {
            assert_eq!(WebUrl::parse(text), None, "{text}");
        }
        assert_eq!(page("https://example.com/login").as_str(), "https://example.com/login");
        assert_eq!(page("http://example.com/").as_str(), "http://example.com/");
    }

    #[test]
    fn hosts_are_normalized() {
        let upper = page("HTTPS://Example.COM/Login?Next=%2F#Top");
        assert_eq!(upper.as_str(), "https://example.com/Login?Next=%2F#Top");
        assert_eq!(upper.host(), "example.com");
        // A trailing dot names the same host (an absolute DNS name).
        let dotted = page("https://example.com./login");
        assert_eq!(dotted.host(), "example.com");
        assert_eq!(dotted.as_str(), "https://example.com/login");
        // Empty labels are not a host anyone signs in on.
        for text in ["https://example.com../", "https://.example.com/", "https://a..example.com/"] {
            assert_eq!(WebUrl::parse(text), None, "{text}");
        }
        // Fullwidth letters and percent escapes map to the same host, as in the browser.
        assert_eq!(page("https://ｅｘａｍｐｌｅ.com/").host(), "example.com");
        assert_eq!(page("https://ex%41mple.com/").host(), "example.com");
    }

    #[test]
    fn idn_in_unicode_and_punycode_is_one_host() {
        let unicode = page("https://例え.jp/");
        let punycode = page("https://xn--r8jz45g.jp/");
        assert_eq!(unicode.host(), "xn--r8jz45g.jp");
        assert_eq!(unicode, punycode);
        assert_eq!(unicode.site(), "xn--r8jz45g.jp");
        // An IDN public suffix: 公司.cn is one, so its subdomains are separate sites.
        assert_eq!(page("https://a.公司.cn/").site(), "a.xn--55qx5d.cn");
        assert!(!page("https://a.公司.cn/").same_site(&page("https://b.xn--55qx5d.cn/")));
        assert!(page("https://www.例子.公司.cn/").same_site(&page("https://xn--fsqu00a.xn--55qx5d.cn/")));
    }

    #[test]
    fn user_info_is_dropped() {
        let with_password = page("https://user:secret@example.com/login");
        assert_eq!(with_password.as_str(), "https://example.com/login");
        assert_eq!(with_password.host(), "example.com");
        // The classic confusion: the host is evil.example, whatever comes before the @.
        let confusing = page("https://example.com@evil.example/");
        assert_eq!(confusing.host(), "evil.example");
        assert_eq!(confusing.as_str(), "https://evil.example/");
    }

    #[test]
    fn ports_are_kept_unless_default() {
        assert_eq!(page("https://example.com:443/").port(), None);
        assert_eq!(page("http://example.com:80/").port(), None);
        assert_eq!(page("http://example.com:443/").port(), Some(443));
        assert_eq!(page("https://example.com:8443/").port(), Some(8443));
        assert_eq!(page("https://example.com:8443/").as_str(), "https://example.com:8443/");
        assert!(page("https://example.com/").is_https());
        assert!(!page("http://example.com/").is_https());
    }

    #[test]
    fn sites_follow_the_public_suffix_list() {
        assert_eq!(page("https://a.b.example.co.uk/").site(), "example.co.uk");
        assert!(page("https://a.example.co.uk/").same_site(&page("http://b.example.co.uk:8080/")));
        // github.io is a (private) public suffix: every user page is a site of its own.
        assert_eq!(page("https://foo.github.io/").site(), "foo.github.io");
        assert!(!page("https://foo.github.io/").same_site(&page("https://bar.github.io/")));
        // A public suffix itself is its own site, separate from what lies under it.
        assert_eq!(page("https://co.uk/").site(), "co.uk");
        assert!(!page("https://co.uk/").same_site(&page("https://example.co.uk/")));
        assert_eq!(page("https://github.io/").site(), "github.io");
        assert!(!page("https://github.io/").same_site(&page("https://foo.github.io/")));
        // Wildcard and exception rules (*.ck, !www.ck).
        assert_eq!(page("https://foo.bar.ck/").site(), "foo.bar.ck");
        assert_eq!(page("https://www.ck/").site(), "www.ck");
        // A suffix the list does not know counts as one label, as browsers do.
        assert_eq!(page("https://a.b.nas.lan/").site(), "nas.lan");
    }

    #[test]
    fn ip_addresses_are_their_own_site() {
        let v4 = page("http://192.168.1.1:8080/admin");
        assert_eq!(v4.host(), "192.168.1.1");
        assert_eq!(v4.site(), "192.168.1.1");
        assert!(v4.is_ip());
        // Not "1.1" for both, as the suffix list alone would have it.
        assert!(!page("http://10.0.1.1/").same_site(&page("http://192.168.1.1/")));
        assert!(page("http://192.168.1.1/").same_site(&page("https://192.168.1.1:8443/")));
        // The WHATWG parser reads hexadecimal and short forms as the address they are.
        assert_eq!(page("https://0x7f.1/").host(), "127.0.0.1");
        let v6 = page("http://[2001:DB8::1]:3000/");
        assert_eq!(v6.host(), "[2001:db8::1]");
        assert_eq!(v6.site(), "[2001:db8::1]");
        assert!(v6.is_ip());
        assert!(!v6.same_site(&page("http://[2001:db8::2]/")));
        assert!(!page("https://example.com/").is_ip());
    }

    #[test]
    fn single_label_hosts_are_their_own_site() {
        assert_eq!(page("http://localhost:3000/").site(), "localhost");
        assert!(page("http://localhost:3000/").same_site(&page("http://localhost:8080/")));
        assert_eq!(page("http://intranet/").site(), "intranet");
        assert!(!page("http://intranet/").same_site(&page("http://localhost/")));
        // Under an unknown suffix the last two labels are the site.
        assert_eq!(page("http://app.localhost/").site(), "app.localhost");
        assert!(!page("http://a.localhost/").same_site(&page("http://b.localhost/")));
    }

    #[test]
    fn domains_run_from_the_host_to_the_site() {
        let domains = |text: &str| page(text).domains().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(domains("https://a.b.example.co.uk/"), ["a.b.example.co.uk", "b.example.co.uk", "example.co.uk"]);
        assert_eq!(domains("https://example.com/"), ["example.com"]);
        assert_eq!(domains("https://co.uk/"), ["co.uk"]);
        assert_eq!(domains("https://x.foo.github.io/"), ["x.foo.github.io", "foo.github.io"]);
        assert_eq!(domains("http://192.168.1.1/"), ["192.168.1.1"]);
        assert_eq!(domains("http://[::1]/"), ["[::1]"]);
        assert_eq!(domains("http://localhost/"), ["localhost"]);
    }

    #[test]
    fn saved_urls_without_a_scheme_read_as_https() {
        let saved = |text: &str| WebUrl::parse_saved(text).unwrap_or_else(|| panic!("{text} should parse"));
        assert_eq!(saved("example.com").as_str(), "https://example.com/");
        assert_eq!(saved("  example.com/login  ").as_str(), "https://example.com/login");
        assert_eq!(saved("Example.com:8443/x").as_str(), "https://example.com:8443/x");
        assert_eq!(saved("localhost:3000").as_str(), "https://localhost:3000/");
        assert_eq!(saved("192.168.1.1:8080/admin").as_str(), "https://192.168.1.1:8080/admin");
        assert_eq!(saved("//example.com/a").as_str(), "https://example.com/a");
        // A `://` further on is no scheme's.
        assert_eq!(saved("localhost:3000/?next=https://example.com").as_str(), "https://localhost:3000/?next=https://example.com");
        assert_eq!(saved("例え.jp").host(), "xn--r8jz45g.jp");
        // A scheme that is there stays.
        assert_eq!(saved("http://example.com").as_str(), "http://example.com/");
        assert_eq!(saved("HTTP://EXAMPLE.COM/A").as_str(), "http://example.com/A");
        assert_eq!(saved("https://user@example.com").as_str(), "https://example.com/");
        // Not a web address, with a scheme or without one.
        for text in [
            "mailto:me@example.com",
            "me@example.com",
            "androidapp://com.example.app",
            "ftp://example.com",
            "about:blank",
            "data:text/html,x",
            "",
            "   ",
            "exa mple.com",
            "https://exa mple.com/",
            "http://[::1",
        ] {
            assert_eq!(WebUrl::parse_saved(text), None, "{text:?}");
        }
    }
}
