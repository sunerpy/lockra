//! Password rules: what a site accepts, in the language Apple designed and proposed to the WHATWG
//! as the `passwordrules` attribute (`required: lower; required: upper; required: digit; allowed:
//! [-().&@?'#,/"+]; max-consecutive: 2; minlength: 8; maxlength: 20`), and the rules Apple's
//! password-manager-resources lists for sites that publish none.
//!
//! Reading follows the reference parser (`parser.rs`): it never fails, and text it cannot read
//! gives the default rules, any ASCII printable character. `max-consecutive`, deprecated upstream
//! on 2026-09-10 (password-manager-resources #1255), stands for both `max-repeating` and
//! `max-sequential`, as the language now defines it.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::address::WebUrl;
use crate::charset::CharacterSet;
use crate::data;
use crate::parser::{Class, Rule, canonical_rules};

/// A site's password rules in canonical form: what a password must contain, may contain, and how
/// long it may be. [`PasswordRules::default`] allows any ASCII printable character, at any length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordRules {
    required: Vec<CharacterSet>,
    allowed: CharacterSet,
    max_repeating: Option<u32>,
    max_sequential: Option<u32>,
    min_length: Option<u32>,
    max_length: Option<u32>,
}

impl Default for PasswordRules {
    fn default() -> Self {
        Self { required: Vec::new(), allowed: CharacterSet::ASCII_PRINTABLE, max_repeating: None, max_sequential: None, min_length: None, max_length: None }
    }
}

impl PasswordRules {
    /// Read rules. Never fails: what cannot be read is left out as the reference parser leaves it
    /// out, and text it cannot read at all gives [`PasswordRules::default`].
    pub fn parse(text: &str) -> Self {
        let mut rules = Self::default();
        let saturated = |value: u64| u32::try_from(value).unwrap_or(u32::MAX);
        let tighter = |held: Option<u32>, value: u64| Some(held.map_or(saturated(value), |held| held.min(saturated(value))));
        for rule in canonical_rules(text) {
            match rule {
                Rule::Required(classes) => rules.required.push(Class::set_of(&classes)),
                Rule::Allowed(classes) => rules.allowed = Class::set_of(&classes),
                Rule::MaxConsecutive(value) => {
                    rules.max_repeating = tighter(rules.max_repeating, value);
                    rules.max_sequential = tighter(rules.max_sequential, value);
                }
                Rule::MaxRepeating(value) => rules.max_repeating = tighter(rules.max_repeating, value),
                Rule::MaxSequential(value) => rules.max_sequential = tighter(rules.max_sequential, value),
                Rule::MinLength(value) => rules.min_length = Some(saturated(value)),
                Rule::MaxLength(value) => rules.max_length = Some(saturated(value)),
            }
        }
        rules
    }

    /// The rules for a password field on a page of `host` (a host or a URL): the page's own
    /// `passwordrules` attribute when it has one, otherwise the bundled entry for the host, then
    /// narrowed by the field's `minlength` and `maxlength`. `None` when there is none of these.
    pub fn for_page(host: &str, attribute: Option<&str>, minlength: Option<u32>, maxlength: Option<u32>) -> Option<Self> {
        // The list keeps entries only for sites that do not publish their rules themselves.
        let rules = match attribute.filter(|attribute| !attribute.trim().is_empty()) {
            Some(attribute) => Some(Self::parse(attribute)),
            None => bundled_password_rules(host),
        };
        let lengths = minlength.is_some_and(|n| n > 0) || maxlength.is_some_and(|n| n > 0);
        match rules {
            Some(rules) => Some(rules.with_lengths(minlength, maxlength)),
            None if lengths => Some(Self::default().with_lengths(minlength, maxlength)),
            None => None,
        }
    }

    /// These rules narrowed by a field's `minlength` and `maxlength` attributes: the larger of
    /// the minimums and the smaller of the maximums count. Zero, as in the rules, is no limit.
    pub fn with_lengths(mut self, minlength: Option<u32>, maxlength: Option<u32>) -> Self {
        if let Some(min) = minlength.filter(|&n| n > 0) {
            self.min_length = Some(self.min_length.map_or(min, |held| held.max(min)));
        }
        if let Some(max) = maxlength.filter(|&n| n > 0) {
            self.max_length = Some(self.max_length.map_or(max, |held| held.min(max)));
        }
        self
    }

    /// The shortest length allowed, in characters.
    pub fn min_length(&self) -> Option<u32> {
        self.min_length
    }

    /// The longest length allowed, in characters.
    pub fn max_length(&self) -> Option<u32> {
        self.max_length
    }

    /// The most identical characters allowed in a row (`aaa` is three): `max-repeating`, or
    /// `max-consecutive`, which the language now defines as both this and [`Self::max_sequential`].
    pub fn max_repeating(&self) -> Option<u32> {
        self.max_repeating
    }

    /// The most characters allowed in a row that step up or down by one within `a`–`z`, `A`–`Z`
    /// or `0`–`9` (`abc` and `321` are three): `max-sequential`, or `max-consecutive`.
    pub fn max_sequential(&self) -> Option<u32> {
        self.max_sequential
    }

    /// Whether `password` meets every rule. A required class without a single character in it
    /// (`required: []`) asks for nothing.
    pub fn is_satisfied_by(&self, password: &str) -> bool {
        let length = password.chars().count();
        let length_ok = self.min_length.is_none_or(|min| length >= min as usize) && self.max_length.is_none_or(|max| length <= max as usize);
        let mut runs = Runs::default();
        length_ok
            && password.chars().all(|c| {
                runs = runs.after(c);
                self.allowed.contains(c) && runs.within(self.max_repeating, self.max_sequential)
            })
            && self.required.iter().all(|class| class.is_empty() || password.chars().any(|c| class.contains(c)))
    }

    /// Whether `c` may follow the characters that left `runs`, as far as the run limits go.
    pub(crate) fn may_follow(&self, runs: Runs, c: char) -> bool {
        runs.after(c).within(self.max_repeating, self.max_sequential)
    }

    /// The classes a password must each have a character of.
    pub(crate) fn required(&self) -> &[CharacterSet] {
        &self.required
    }

    /// The characters a password may have.
    pub(crate) fn allowed(&self) -> CharacterSet {
        self.allowed
    }
}

/// The bundled rules for `host` (a host or a URL) from Apple's password-manager-resources: the
/// entry for the host itself, else for the nearest parent domain down to the registrable domain,
/// as the list defines its keys (`a.example.com` covers `x.a.example.com` but not
/// `b.example.com`; an `exact-domain-match-only` entry covers only its own host).
pub fn bundled_password_rules(host: &str) -> Option<PasswordRules> {
    lookup(&BUNDLED, &WebUrl::parse_saved(host)?)
}

/// Domain to its rules and whether they cover that host only.
type Quirks = HashMap<String, (PasswordRules, bool)>;

/// An entry of `password-rules.json`, as `quirks/schemas/password-rules-schema.json` at the pinned
/// commit defines it; unknown keys fail the list (and the tests) rather than vanish.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Quirk {
    #[serde(rename = "password-rules")]
    rules: String,
    #[serde(rename = "exact-domain-match-only", default)]
    exact: bool,
}

pub(crate) fn bundled_quirks(json: &str) -> Result<Quirks, serde_json::Error> {
    let entries: HashMap<String, Quirk> = serde_json::from_str(json)?;
    Ok(entries.into_iter().map(|(domain, quirk)| (domain, (PasswordRules::parse(&quirk.rules), quirk.exact))).collect())
}

/// The bundled list, parsed on first use. The file is pinned and parsed in the tests, so the
/// empty fallback is never what a build ships with.
static BUNDLED: LazyLock<Quirks> = LazyLock::new(|| bundled_quirks(data::PASSWORD_RULES).unwrap_or_default());

fn lookup(quirks: &Quirks, url: &WebUrl) -> Option<PasswordRules> {
    url.domains().find_map(|domain| quirks.get(domain).filter(|(_, exact)| !exact || domain == url.host())).map(|(rules, _)| rules.clone())
}

/// The runs that end at the last character of a password: of identical characters, and of
/// characters stepping up or down by one within `a`–`z`, `A`–`Z` or `0`–`9`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Runs {
    last: Option<char>,
    repeating: u32,
    ascending: u32,
    descending: u32,
}

impl Runs {
    /// The runs once `c` follows.
    pub(crate) fn after(self, c: char) -> Self {
        let step = self.last.and_then(|last| step(last, c));
        let longer = |run: u32, matches: bool| if matches { run.saturating_add(1) } else { 1 };
        Self {
            last: Some(c),
            repeating: longer(self.repeating, self.last == Some(c)),
            ascending: longer(self.ascending, step == Some(1)),
            descending: longer(self.descending, step == Some(-1)),
        }
    }

    fn within(self, max_repeating: Option<u32>, max_sequential: Option<u32>) -> bool {
        max_repeating.is_none_or(|max| self.repeating <= max) && max_sequential.is_none_or(|max| self.ascending.max(self.descending) <= max)
    }
}

/// `to - from` when both are in one of the ranges `a`–`z`, `A`–`Z`, `0`–`9`.
fn step(from: char, to: char) -> Option<i64> {
    let same_range = [CharacterSet::LOWER, CharacterSet::UPPER, CharacterSet::DIGIT].iter().any(|range| range.contains(from) && range.contains(to));
    same_range.then(|| i64::from(u32::from(to)) - i64::from(u32::from(from)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_read_into_their_limits() {
        let rules = PasswordRules::parse(
            "required: lower; required: upper; required: digit; allowed: [-().&@?'#,/\"+]; max-consecutive: 2; minlength: 8; maxlength: 20",
        );
        assert_eq!((rules.min_length(), rules.max_length()), (Some(8), Some(20)));
        // max-consecutive stands for both runs, the stricter value winning.
        assert_eq!((rules.max_repeating(), rules.max_sequential()), (Some(2), Some(2)));
        let rules = PasswordRules::parse("max-consecutive: 3; max-repeating: 2");
        assert_eq!((rules.max_repeating(), rules.max_sequential()), (Some(2), Some(3)));
        let rules = PasswordRules::parse("max-sequential: 4; max-consecutive: 5; max-sequential: 2");
        assert_eq!((rules.max_repeating(), rules.max_sequential()), (Some(5), Some(2)));
        // Zero is no value, and a length past u32 stays as long as it gets.
        let rules = PasswordRules::parse("minlength: 0; maxlength: 99999999999999999999999");
        assert_eq!((rules.min_length(), rules.max_length()), (None, Some(u32::MAX)));
        assert_eq!(PasswordRules::parse(""), PasswordRules::default());
        assert_eq!(PasswordRules::parse("dummy: upper"), PasswordRules::default());
        assert_eq!(PasswordRules::default().allowed(), CharacterSet::ASCII_PRINTABLE);
        assert!(PasswordRules::default().required().is_empty());
    }

    #[test]
    fn a_password_meets_every_rule_or_fails() {
        let rules = PasswordRules::parse(
            "required: lower; required: upper; required: digit; allowed: [-().&@?'#,/\"+]; max-consecutive: 2; minlength: 8; maxlength: 20",
        );
        assert!(rules.is_satisfied_by("Ab1(x9Z-"));
        assert!(rules.is_satisfied_by("aaBB1-\"x"));
        assert!(!rules.is_satisfied_by("Ab1(x9Z"), "too short");
        assert!(!rules.is_satisfied_by("Ab1(x9Z-Ab1(x9Z-Ab1(x"), "too long");
        assert!(!rules.is_satisfied_by("ab1(x9z-"), "no upper");
        assert!(!rules.is_satisfied_by("Ab1(x9Z!"), "! is not allowed");
        assert!(!rules.is_satisfied_by("Ab1(xxxZ-"), "three of a kind");
        assert!(!rules.is_satisfied_by("Xabc1(9Z-"), "abc runs up");
        assert!(rules.is_satisfied_by("Abc1(x9Z-"), "A and b are in different ranges: bc is a run of two");
        assert!(!rules.is_satisfied_by("Ab321(xZ-"), "321 runs down");
        assert!(!rules.is_satisfied_by("Ab1(xZé9-"), "é is not ASCII");
        // Steps count only within a-z, A-Z and 0-9, and only in one direction at a time.
        let runs = PasswordRules::parse("max-sequential: 2; max-repeating: 2");
        assert!(runs.is_satisfied_by("abab"));
        assert!(runs.is_satisfied_by("xyZ[\\]89:"));
        assert!(runs.is_satisfied_by("aab"));
        assert!(!runs.is_satisfied_by("xyz"));
        assert!(!runs.is_satisfied_by("CBA"));
        assert!(!runs.is_satisfied_by("789"));
        assert!(!runs.is_satisfied_by("aaa"));
        // Unicode allows anything; an empty required class asks for nothing.
        assert!(PasswordRules::parse("allowed: unicode").is_satisfied_by("pässwörd 密码"));
        assert!(!PasswordRules::default().is_satisfied_by("pässwörd"));
        assert!(PasswordRules::parse("required: []; minlength: 1").is_satisfied_by("x"));
        assert!(!PasswordRules::parse("minlength: 1").is_satisfied_by(""));
    }

    #[test]
    fn page_lengths_narrow_the_rules() {
        let rules = PasswordRules::parse("minlength: 8; maxlength: 20").with_lengths(Some(10), Some(16));
        assert_eq!((rules.min_length(), rules.max_length()), (Some(10), Some(16)));
        let rules = PasswordRules::parse("minlength: 8; maxlength: 20").with_lengths(Some(6), Some(32));
        assert_eq!((rules.min_length(), rules.max_length()), (Some(8), Some(20)));
        let rules = PasswordRules::default().with_lengths(Some(12), None);
        assert_eq!((rules.min_length(), rules.max_length()), (Some(12), None));
        let rules = PasswordRules::default().with_lengths(Some(0), Some(0));
        assert_eq!((rules.min_length(), rules.max_length()), (None, None));
    }

    #[test]
    fn bundled_rules_are_found_by_host_then_parent_domain() {
        let found = |host: &str| bundled_password_rules(host).map(|rules| (rules.min_length(), rules.max_length()));
        // "163.com": "minlength: 6; maxlength: 16;"
        assert_eq!(found("163.com"), Some((Some(6), Some(16))));
        assert_eq!(found("mail.163.com"), Some((Some(6), Some(16))));
        assert_eq!(found("https://reg.mail.163.com/login?x=1"), Some((Some(6), Some(16))));
        assert_eq!(found("MAIL.163.COM."), Some((Some(6), Some(16))));
        // "account.xiaomi.com" has rules of its own, apart from "xiaomi.com".
        let account = bundled_password_rules("account.xiaomi.com").unwrap();
        let site = bundled_password_rules("www.xiaomi.com").unwrap();
        assert_ne!(account, site);
        assert_eq!(bundled_password_rules("x.account.xiaomi.com"), Some(account));
        assert_eq!(bundled_password_rules("xiaomi.com"), Some(site));
        // Nothing for others, nor for what is no web host.
        assert_eq!(found("example.com"), None);
        assert_eq!(found("163.com.example.net"), None);
        assert_eq!(found("http://192.168.1.1/"), None);
        assert_eq!(found(""), None);
    }

    #[test]
    fn exact_entries_cover_only_their_host() {
        let quirks = bundled_quirks(
            r#"{"a.example.com": {"password-rules": "minlength: 9;", "exact-domain-match-only": true}, "example.com": {"password-rules": "minlength: 7;"}}"#,
        )
        .unwrap();
        let length = |host: &str| lookup(&quirks, &WebUrl::parse_saved(host).unwrap()).and_then(|rules| rules.min_length());
        assert_eq!(length("a.example.com"), Some(9));
        assert_eq!(length("x.a.example.com"), Some(7));
        assert_eq!(length("b.example.com"), Some(7));
        assert_eq!(length("example.org"), None);
    }

    #[test]
    fn the_bundled_rules_follow_their_schema() {
        let quirks = bundled_quirks(data::PASSWORD_RULES).unwrap();
        // 436 entries at the pinned commit, none of them exact-only, each with rules that read.
        assert_eq!(quirks.len(), 436);
        assert!(quirks.values().all(|(rules, exact)| !exact && *rules != PasswordRules::default()));
        assert!(bundled_quirks(r#"{"example.com": {"password-rules": "minlength: 7;", "other": 1}}"#).is_err());
        assert!(bundled_quirks(r#"{"example.com": {"exact-domain-match-only": true}}"#).is_err());
    }

    #[test]
    fn the_page_attribute_comes_before_the_bundled_entry() {
        // A page that says nothing gets the bundled rules, narrowed by its own lengths.
        let rules = PasswordRules::for_page("https://mail.163.com/", None, Some(8), Some(12)).unwrap();
        assert_eq!((rules.min_length(), rules.max_length()), (Some(8), Some(12)));
        // The page's attribute wins over the list; a blank one says nothing.
        let rules = PasswordRules::for_page("163.com", Some("minlength: 10; required: digit"), None, None).unwrap();
        assert_eq!((rules.min_length(), rules.max_length()), (Some(10), None));
        assert_eq!(PasswordRules::for_page("163.com", Some("  "), None, None).unwrap().max_length(), Some(16));
        // Lengths alone are rules too; nothing at all is no rules.
        let rules = PasswordRules::for_page("example.com", None, None, Some(30)).unwrap();
        assert_eq!(rules, PasswordRules::default().with_lengths(None, Some(30)));
        assert_eq!(PasswordRules::for_page("example.com", None, None, None), None);
        assert_eq!(PasswordRules::for_page("example.com", None, Some(0), None), None);
    }
}
