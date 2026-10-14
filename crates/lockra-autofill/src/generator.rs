//! The password generator: random passwords from character groups, and passphrases from the
//! EFF's large wordlist. Either can be held to a site's [`PasswordRules`].

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::charset::CharacterSet;
use crate::random::{RandomSource, RandomUnavailable};
use crate::rules::{PasswordRules, Runs};
use crate::wordlist::{self, LONGEST_WORD};

/// The lengths a random password may have; options outside are brought inside.
pub const PASSWORD_LENGTHS: RangeInclusive<u32> = 4..=128;

/// The word counts a passphrase may have; options outside are brought inside.
pub const PASSPHRASE_WORDS: RangeInclusive<u32> = 3..=20;

/// The most characters of a separator that are kept.
pub const SEPARATOR_MAX_CHARS: usize = 3;

/// A random password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct PasswordOptions {
    /// Characters, within [`PASSWORD_LENGTHS`]; site rules may lengthen or shorten it.
    pub length: u32,
    /// `a`–`z`.
    pub lower: bool,
    /// `A`–`Z`.
    pub upper: bool,
    /// `0`–`9`.
    pub digits: bool,
    /// The 32 ASCII punctuation characters: every printable one that is no letter, digit or space.
    pub symbols: bool,
    /// Leave out characters that are easy to mistake for one another: `I l 1 | O 0`.
    pub avoid_ambiguous: bool,
    /// Characters never to use.
    pub exclude: String,
}

impl Default for PasswordOptions {
    fn default() -> Self {
        Self { length: 20, lower: true, upper: true, digits: true, symbols: true, avoid_ambiguous: false, exclude: String::new() }
    }
}

/// A passphrase of words from the EFF's large wordlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct PassphraseOptions {
    /// Words, within [`PASSPHRASE_WORDS`].
    pub words: u32,
    /// Between the words: up to [`SEPARATOR_MAX_CHARS`] characters, control characters dropped.
    pub separator: String,
    /// Each word begins with a capital.
    pub capitalize: bool,
    /// One word, chosen at random, ends with a digit.
    pub number: bool,
}

impl Default for PassphraseOptions {
    fn default() -> Self {
        Self { words: 6, separator: "-".to_owned(), capitalize: false, number: false }
    }
}

/// What to generate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneratorOptions {
    /// A random password.
    Password(PasswordOptions),
    /// A passphrase.
    Passphrase(PassphraseOptions),
}

impl Default for GeneratorOptions {
    fn default() -> Self {
        Self::Password(PasswordOptions::default())
    }
}

/// A generated password.
pub struct Generated {
    /// The password, wiped from memory when dropped.
    pub password: Zeroizing<String>,
    /// An estimate of its entropy in bits, rounded down: what the random choices made it.
    pub bits: u32,
    /// Rules were given, and the password meets them (without rules this is `false`).
    pub rules_met: bool,
}

impl std::fmt::Debug for Generated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Generated").field("password", &"<hidden>").field("bits", &self.bits).field("rules_met", &self.rules_met).finish()
    }
}

/// Why nothing was generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GeneratorError {
    /// No password meets the rules with these options: the rules contradict themselves (a
    /// minimum length above the maximum), ask for what the options exclude, or cannot be met
    /// by a passphrase.
    #[error("no password meets the site's rules with these options")]
    RulesUnsatisfiable,
    /// The options leave no character to choose from.
    #[error("the options leave no character to choose from")]
    NoCharacters,
    /// The system's random number generator failed.
    #[error(transparent)]
    Random(#[from] RandomUnavailable),
}

/// Characters that are easy to mistake for one another.
const AMBIGUOUS: &str = "Il1|O0";

/// Tries before rules count as impossible to meet. A random password's try fails only when the
/// run limits leave no character for a position, which takes a contrived alphabet to happen
/// often; a passphrase is drawn whole and checked.
const ATTEMPTS: usize = 100;

/// Generate a password or passphrase with `options`, meeting `rules` when there are any.
///
/// With rules, the rules come first and the options give way where they disagree: the length
/// moves into the rules' range, and the chosen groups narrow the characters the rules allow
/// unless nothing would be left (then all of those are used). Excluded and ambiguous characters
/// are never used; when that leaves the rules unmet the result is
/// [`GeneratorError::RulesUnsatisfiable`]. A space is used only where the rules leave no other
/// character, as for `required: [ ]`.
pub fn generate(options: &GeneratorOptions, rules: Option<&PasswordRules>, random: &mut (impl RandomSource + ?Sized)) -> Result<Generated, GeneratorError> {
    let (password, bits) = match options {
        GeneratorOptions::Password(options) => random_password(options, rules, random)?,
        GeneratorOptions::Passphrase(options) => passphrase_for(options, rules, random)?,
    };
    Ok(Generated { password, bits: bits.floor() as u32, rules_met: rules.is_some() })
}

/// What a random password is drawn from: its length, the characters of an ordinary position,
/// and the classes that each get a position of their own.
struct Plan {
    length: u32,
    alphabet: CharacterSet,
    required: Vec<CharacterSet>,
}

impl Plan {
    fn new(options: &PasswordOptions, rules: Option<&PasswordRules>) -> Result<Self, GeneratorError> {
        let mut removed = CharacterSet::of(options.exclude.chars());
        if options.avoid_ambiguous {
            removed = removed.union(CharacterSet::of(AMBIGUOUS.chars()));
        }
        let groups: Vec<CharacterSet> = [
            (options.lower, CharacterSet::LOWER),
            (options.upper, CharacterSet::UPPER),
            (options.digits, CharacterSet::DIGIT),
            (options.symbols, CharacterSet::PUNCTUATION),
        ]
        .into_iter()
        .filter(|&(chosen, _)| chosen)
        .map(|(_, group)| group.without(removed))
        .filter(|group| !group.is_empty())
        .collect();
        let chosen = groups.iter().fold(CharacterSet::EMPTY, |all, group| all.union(*group));
        let Some(rules) = rules else {
            if chosen.is_empty() {
                return Err(GeneratorError::NoCharacters);
            }
            let length = options.length.clamp(*PASSWORD_LENGTHS.start(), *PASSWORD_LENGTHS.end());
            return Ok(Self { length, alphabet: chosen, required: groups });
        };

        let space = CharacterSet::of([' ']);
        let site_all = rules.allowed().without(removed);
        let site = Some(site_all.without(space)).filter(|site| !site.is_empty()).unwrap_or(site_all);
        if site.is_empty() {
            return Err(GeneratorError::RulesUnsatisfiable);
        }
        let alphabet = Some(chosen.intersection(site)).filter(|both| !both.is_empty()).unwrap_or(site);
        let mut hard = Vec::new();
        for class in rules.required().iter().filter(|class| !class.is_empty()) {
            let usable = Some(class.printable().intersection(site)).filter(|usable| !usable.is_empty()).unwrap_or(class.printable().intersection(site_all));
            if usable.is_empty() {
                return Err(GeneratorError::RulesUnsatisfiable);
            }
            hard.push(usable);
        }
        let soft = groups.iter().map(|group| group.intersection(alphabet)).filter(|group| !group.is_empty()).collect();
        let shortest = rules.min_length().unwrap_or(0).max(*PASSWORD_LENGTHS.start());
        let longest = rules.max_length().unwrap_or(u32::MAX).min(*PASSWORD_LENGTHS.end());
        if shortest > longest {
            return Err(GeneratorError::RulesUnsatisfiable);
        }
        let length = options.length.clamp(shortest, longest);
        let required = requirements(hard, soft, length).ok_or(GeneratorError::RulesUnsatisfiable)?;
        Ok(Self { length, alphabet, required })
    }
}

/// The classes that get a position of their own. A class that contains another is met by that
/// one's character and needs no position; the site's (`hard`) classes all stay, and the options'
/// (`soft`) groups give way, the largest first, when the password is too short for every one.
/// `None` when the site's classes alone do not fit.
fn requirements(hard: Vec<CharacterSet>, soft: Vec<CharacterSet>, length: u32) -> Option<Vec<CharacterSet>> {
    let mut all: Vec<(CharacterSet, bool)> = hard.into_iter().map(|class| (class, true)).chain(soft.into_iter().map(|class| (class, false))).collect();
    all.sort_by_key(|&(class, _)| class.len());
    let mut kept: Vec<(CharacterSet, bool)> = Vec::new();
    for (class, is_hard) in all {
        match kept.iter_mut().find(|(held, _)| held.is_subset(class)) {
            Some(held) => held.1 |= is_hard,
            None => kept.push((class, is_hard)),
        }
    }
    let mut room = (length as usize).checked_sub(kept.iter().filter(|&&(_, is_hard)| is_hard).count())?;
    let kept = kept.into_iter().filter(|&(_, is_hard)| {
        let keep = is_hard || room > 0;
        if !is_hard && keep {
            room -= 1;
        }
        keep
    });
    Some(kept.map(|(class, _)| class).collect())
}

fn random_password(
    options: &PasswordOptions,
    rules: Option<&PasswordRules>,
    random: &mut (impl RandomSource + ?Sized),
) -> Result<(Zeroizing<String>, f64), GeneratorError> {
    let plan = Plan::new(options, rules)?;
    let length = plan.length as usize;
    for _ in 0..ATTEMPTS {
        // A distinct random position for each required class, by a partial shuffle.
        let mut positions: Vec<usize> = (0..length).collect();
        let mut class_at = vec![plan.alphabet; length];
        for (index, class) in plan.required.iter().enumerate() {
            let pick = index + random.below((length - index) as u32)? as usize;
            positions.swap(index, pick);
            class_at[positions[index]] = *class;
        }
        let mut password = Zeroizing::new(String::with_capacity(length));
        let mut runs = Runs::default();
        let mut bits = 0.0;
        for class in class_at {
            let candidates = match rules {
                Some(rules) => CharacterSet::of(class.chars().filter(|&c| rules.may_follow(runs, c))),
                None => class,
            };
            // The run limits left nothing for this position: try again from the start.
            let Some(c) = candidates.pick(random)? else { break };
            bits += f64::from(candidates.len()).log2();
            password.push(c);
            runs = runs.after(c);
        }
        if password.len() == length && rules.is_none_or(|rules| rules.is_satisfied_by(&password)) {
            return Ok((password, bits));
        }
    }
    Err(GeneratorError::RulesUnsatisfiable)
}

fn passphrase_for(
    options: &PassphraseOptions,
    rules: Option<&PasswordRules>,
    random: &mut (impl RandomSource + ?Sized),
) -> Result<(Zeroizing<String>, f64), GeneratorError> {
    let Some(rules) = rules else { return Ok(passphrase(options, random)?) };
    for _ in 0..ATTEMPTS {
        let (text, bits) = passphrase(options, random)?;
        if rules.is_satisfied_by(&text) {
            return Ok((text, bits));
        }
    }
    Err(GeneratorError::RulesUnsatisfiable)
}

fn passphrase(options: &PassphraseOptions, random: &mut (impl RandomSource + ?Sized)) -> Result<(Zeroizing<String>, f64), RandomUnavailable> {
    let words = options.words.clamp(*PASSPHRASE_WORDS.start(), *PASSPHRASE_WORDS.end());
    let separator: String = options.separator.chars().filter(|c| !c.is_control()).take(SEPARATOR_MAX_CHARS).collect();
    let list = wordlist::words();
    let digit = if options.number { Some((random.below(words)?, random.below(10)?)) } else { None };
    // Room for the longest words, so the text is never moved (and copied) as it grows.
    let mut text = Zeroizing::new(String::with_capacity(words as usize * (LONGEST_WORD + separator.len()) + 1));
    for index in 0..words {
        if index > 0 {
            text.push_str(&separator);
        }
        let word = list[random.below(list.len() as u32)? as usize];
        let mut chars = word.chars();
        if let Some(first) = chars.next().filter(|_| options.capitalize) {
            text.push(first.to_ascii_uppercase());
            text.push_str(chars.as_str());
        } else {
            text.push_str(word);
        }
        if let Some((at, value)) = digit
            && at == index
        {
            text.push(char::from(b'0' + value as u8));
        }
    }
    let mut bits = f64::from(words) * (list.len() as f64).log2();
    if options.number {
        bits += (10.0 * f64::from(words)).log2();
    }
    Ok((text, bits))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use proptest::prelude::*;

    use super::*;
    use crate::random::OsRandom;
    use crate::random::testing::{Scripted, Seeded};
    use crate::wordlist;

    const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
    const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const DIGITS: &str = "0123456789";
    const SYMBOLS: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

    fn password(options: &PasswordOptions, rules: Option<&PasswordRules>, seed: u64) -> Result<Generated, GeneratorError> {
        generate(&GeneratorOptions::Password(options.clone()), rules, &mut Seeded(seed))
    }

    fn passphrase(options: &PassphraseOptions, rules: Option<&PasswordRules>, seed: u64) -> Result<Generated, GeneratorError> {
        generate(&GeneratorOptions::Passphrase(options.clone()), rules, &mut Seeded(seed))
    }

    fn only(lower: bool, upper: bool, digits: bool, symbols: bool) -> PasswordOptions {
        PasswordOptions { lower, upper, digits, symbols, ..PasswordOptions::default() }
    }

    #[test]
    fn options_have_defaults_and_stable_names() {
        assert_eq!(GeneratorOptions::default(), GeneratorOptions::Password(PasswordOptions::default()));
        let json = serde_json::to_value(GeneratorOptions::default()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "password", "length": 20, "lower": true, "upper": true, "digits": true, "symbols": true, "avoid_ambiguous": false, "exclude": ""})
        );
        let json = serde_json::to_value(GeneratorOptions::Passphrase(PassphraseOptions::default())).unwrap();
        assert_eq!(json, serde_json::json!({"kind": "passphrase", "words": 6, "separator": "-", "capitalize": false, "number": false}));
        // Missing fields take their defaults.
        let read: GeneratorOptions = serde_json::from_str(r#"{"kind": "passphrase", "words": 4}"#).unwrap();
        assert_eq!(read, GeneratorOptions::Passphrase(PassphraseOptions { words: 4, ..PassphraseOptions::default() }));
    }

    #[test]
    fn a_password_has_its_length_and_every_chosen_group() {
        for seed in 0..200 {
            let generated = password(&PasswordOptions::default(), None, seed).unwrap();
            let text = generated.password.as_str();
            assert_eq!(text.chars().count(), 20);
            for group in [LOWER, UPPER, DIGITS, SYMBOLS] {
                assert!(text.chars().any(|c| group.contains(c)), "{group} missing from {text}");
            }
            assert!(text.chars().all(|c| c.is_ascii_graphic()), "{text}");
            assert!(!generated.rules_met);
        }
        // Four groups fit in the shortest password, one character each.
        let shortest = password(&PasswordOptions { length: 4, ..PasswordOptions::default() }, None, 3).unwrap();
        for group in [LOWER, UPPER, DIGITS, SYMBOLS] {
            assert_eq!(shortest.password.chars().filter(|&c| group.contains(c)).count(), 1);
        }
        let digits = password(&only(false, false, true, false), None, 1).unwrap();
        assert!(digits.password.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn lengths_are_brought_within_bounds() {
        let length = |length: u32| password(&PasswordOptions { length, ..PasswordOptions::default() }, None, 9).unwrap().password.len();
        assert_eq!(length(0), 4);
        assert_eq!(length(3), 4);
        assert_eq!(length(4), 4);
        assert_eq!(length(128), 128);
        assert_eq!(length(129), 128);
        assert_eq!(length(u32::MAX), 128);
    }

    #[test]
    fn excluded_and_ambiguous_characters_never_appear() {
        let options = PasswordOptions { length: 128, avoid_ambiguous: true, exclude: "aeiou#\"\\é".to_owned(), ..PasswordOptions::default() };
        for seed in 0..100 {
            let generated = password(&options, None, seed).unwrap();
            assert!(!generated.password.chars().any(|c| "Il1|O0aeiou#\"\\".contains(c)), "{}", generated.password.as_str());
        }
        // A group whose every character is excluded is no longer asked for.
        let options = PasswordOptions { exclude: DIGITS.to_owned(), ..PasswordOptions::default() };
        assert!(!password(&options, None, 5).unwrap().password.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn no_character_left_is_an_error() {
        assert_eq!(password(&only(false, false, false, false), None, 1).unwrap_err(), GeneratorError::NoCharacters);
        let all = format!("{LOWER}{UPPER}{DIGITS}{SYMBOLS}");
        assert_eq!(password(&PasswordOptions { exclude: all, ..PasswordOptions::default() }, None, 1).unwrap_err(), GeneratorError::NoCharacters);
    }

    #[test]
    fn the_estimate_counts_the_choices() {
        // Ten digits: log2(10) each.
        assert_eq!(password(&PasswordOptions { length: 10, ..only(false, false, true, false) }, None, 1).unwrap().bits, 33);
        // The four required characters come from their group, the other 16 from all 94.
        let expected = (26f64.log2() * 2.0 + 10f64.log2() + 32f64.log2() + 16.0 * 94f64.log2()).floor() as u32;
        assert_eq!(password(&PasswordOptions::default(), None, 1).unwrap().bits, expected);
        // Six words: log2(7776) each; the digit adds log2(10) and the word it goes on.
        assert_eq!(passphrase(&PassphraseOptions::default(), None, 1).unwrap().bits, 77);
        assert_eq!(
            passphrase(&PassphraseOptions { number: true, ..PassphraseOptions::default() }, None, 1).unwrap().bits,
            (6.0 * 7776f64.log2() + 60f64.log2()).floor() as u32
        );
    }

    #[test]
    fn site_rules_are_met() {
        let rules = PasswordRules::parse(
            "required: lower; required: upper; required: digit; allowed: [-().&@?'#,/\"+]; max-consecutive: 2; minlength: 8; maxlength: 20",
        );
        for seed in 0..300 {
            let generated = password(&PasswordOptions { length: 32, ..PasswordOptions::default() }, Some(&rules), seed).unwrap();
            assert!(rules.is_satisfied_by(&generated.password), "{}", generated.password.as_str());
            assert_eq!(generated.password.len(), 20, "the longest the site takes");
            assert!(generated.rules_met);
        }
        // The rules' minimum lengthens a short password.
        let generated = password(&PasswordOptions { length: 4, ..PasswordOptions::default() }, Some(&rules), 2).unwrap();
        assert_eq!(generated.password.len(), 8);
    }

    #[test]
    fn every_bundled_rule_can_be_met() {
        let quirks = crate::rules::bundled_quirks(crate::data::PASSWORD_RULES).unwrap();
        let mut domains: Vec<&String> = quirks.keys().collect();
        domains.sort();
        for (seed, domain) in domains.into_iter().enumerate() {
            let (rules, _) = &quirks[domain];
            for options in [PasswordOptions::default(), PasswordOptions { length: 64, avoid_ambiguous: true, ..only(true, false, true, false) }] {
                let generated = password(&options, Some(rules), seed as u64).unwrap_or_else(|error| panic!("{domain}: {error}"));
                assert!(rules.is_satisfied_by(&generated.password), "{domain}: {}", generated.password.as_str());
            }
        }
    }

    #[test]
    fn the_rules_come_before_the_options() {
        // The site allows only special characters, which the options left out: they are used.
        let rules = PasswordRules::parse("required: special; minlength: 12");
        for seed in 0..50 {
            let generated = password(&only(true, false, false, false), Some(&rules), seed).unwrap();
            assert!(rules.is_satisfied_by(&generated.password));
            assert!(generated.password.chars().all(|c| c.is_ascii_lowercase() || " !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~".contains(c)));
        }
        // Only digits allowed, letters chosen: the site's characters are used.
        let pin = PasswordRules::parse("allowed: digit; minlength: 6; maxlength: 6; max-consecutive: 3");
        let generated = password(&only(true, true, false, false), Some(&pin), 4).unwrap();
        assert!(pin.is_satisfied_by(&generated.password) && generated.password.chars().all(|c| c.is_ascii_digit()));
        // The options' groups narrow what the site allows when they can.
        let wide = PasswordRules::parse("allowed: ascii-printable");
        let generated = password(&only(false, false, true, false), Some(&wide), 4).unwrap();
        assert!(generated.password.chars().all(|c| c.is_ascii_digit()));
        // A space only where nothing else will do.
        for seed in 0..50 {
            assert!(!password(&PasswordOptions::default(), Some(&wide), seed).unwrap().password.contains(' '));
            assert!(!password(&only(true, false, false, false), Some(&rules), seed).unwrap().password.contains(' '));
        }
        let space = PasswordRules::parse("required: [ ]; required: digit; allowed: lower");
        let generated = password(&PasswordOptions::default(), Some(&space), 1).unwrap();
        assert!(space.is_satisfied_by(&generated.password) && generated.password.contains(' '));
    }

    #[test]
    fn tight_rules_are_met_by_the_one_way_left() {
        // `a` and `c` may not repeat, and are no sequence: they alternate.
        let rules = PasswordRules::parse("allowed: [ac]; max-consecutive: 1; minlength: 10; maxlength: 10");
        for seed in 0..20 {
            let generated = password(&PasswordOptions::default(), Some(&rules), seed).unwrap();
            assert!(matches!(generated.password.as_str(), "acacacacac" | "cacacacaca"), "{}", generated.password.as_str());
            assert_eq!(generated.bits, 1);
        }
        // One required character from a class of one.
        let rules = PasswordRules::parse("required: [!]; allowed: digit; max-repeating: 1");
        for seed in 0..50 {
            let generated = password(&PasswordOptions { length: 12, ..PasswordOptions::default() }, Some(&rules), seed).unwrap();
            assert!(rules.is_satisfied_by(&generated.password) && generated.password.contains('!'));
        }
    }

    #[test]
    fn rules_that_cannot_be_met_are_an_error() {
        let unsatisfiable = |rules: &str, options: &PasswordOptions| password(options, Some(&PasswordRules::parse(rules)), 1).unwrap_err();
        let default = PasswordOptions::default();
        for rules in [
            "minlength: 200",
            "maxlength: 3",
            "minlength: 20; maxlength: 10",
            // `ab` is a sequence and `aa` a repeat: no two characters can follow each other.
            "allowed: [ab]; max-consecutive: 1",
            "required: [a]; required: [b]; required: [c]; required: [d]; required: [e]; maxlength: 4",
        ] {
            assert_eq!(unsatisfiable(rules, &default), GeneratorError::RulesUnsatisfiable, "{rules}");
        }
        let no_bang = PasswordOptions { exclude: "!".to_owned(), ..default.clone() };
        assert_eq!(unsatisfiable("allowed: [!]", &no_bang), GeneratorError::RulesUnsatisfiable);
        assert_eq!(unsatisfiable("required: [!]", &no_bang), GeneratorError::RulesUnsatisfiable);
        let no_digits = PasswordOptions { exclude: DIGITS.to_owned(), ..default };
        assert_eq!(unsatisfiable("required: digit", &no_digits), GeneratorError::RulesUnsatisfiable);
        // Other characters are left, but none of the required class.
        assert_eq!(unsatisfiable("required: digit; allowed: lower", &no_digits), GeneratorError::RulesUnsatisfiable);
    }

    #[test]
    fn a_required_class_without_characters_asks_for_nothing() {
        let rules = PasswordRules::parse("required: []; required: upper");
        let generated = password(&PasswordOptions::default(), Some(&rules), 1).unwrap();
        assert!(rules.is_satisfied_by(&generated.password) && generated.password.chars().any(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn passphrases_are_words_from_the_list() {
        let words = wordlist::words();
        for seed in 0..100 {
            let generated = passphrase(&PassphraseOptions::default(), None, seed).unwrap();
            // Words with a hyphen of their own make the split ambiguous; check by the list.
            assert!(is_words(&generated.password, "-", 6, words), "{}", generated.password.as_str());
            assert!(!generated.rules_met);
        }
        let options = PassphraseOptions { words: 4, separator: " ".to_owned(), capitalize: true, number: true };
        for seed in 0..100 {
            let generated = passphrase(&options, None, seed).unwrap();
            let parts: Vec<&str> = generated.password.split(' ').collect();
            assert_eq!(parts.len(), 4, "{}", generated.password.as_str());
            assert!(parts.iter().all(|part| part.starts_with(|c: char| c.is_ascii_uppercase())));
            assert_eq!(parts.iter().filter(|part| part.ends_with(|c: char| c.is_ascii_digit())).count(), 1);
            for part in parts {
                let word = part.trim_end_matches(|c: char| c.is_ascii_digit()).to_ascii_lowercase();
                assert!(words.contains(&word.as_str()), "{part}");
            }
        }
    }

    /// Whether `text` is `count` (at least one) words of `list` joined by `separator`.
    fn is_words(text: &str, separator: &str, count: usize, list: &[&str]) -> bool {
        list.iter().any(|word| {
            text.strip_prefix(word).is_some_and(|rest| {
                if count == 1 { rest.is_empty() } else { rest.strip_prefix(separator).is_some_and(|rest| is_words(rest, separator, count - 1, list)) }
            })
        })
    }

    #[test]
    fn passphrase_options_are_brought_within_bounds() {
        let count = |words: u32| {
            passphrase(&PassphraseOptions { words, separator: "+".to_owned(), ..PassphraseOptions::default() }, None, 5).unwrap().password.split('+').count()
        };
        assert_eq!(count(0), 3);
        assert_eq!(count(3), 3);
        assert_eq!(count(20), 20);
        assert_eq!(count(500), 20);
        // At most three characters of a separator, and no control characters.
        let generated = passphrase(&PassphraseOptions { words: 3, separator: "\n.\t:-=+".to_owned(), ..PassphraseOptions::default() }, None, 5).unwrap();
        assert!(is_words(&generated.password, ".:-", 3, wordlist::words()), "{}", generated.password.as_str());
        let generated = passphrase(&PassphraseOptions { words: 3, separator: String::new(), ..PassphraseOptions::default() }, None, 5).unwrap();
        assert!(is_words(&generated.password, "", 3, wordlist::words()));
    }

    #[test]
    fn passphrases_meet_rules_or_fail() {
        // Without an `allowed` property only the required classes are allowed: allow the rest.
        let rules = PasswordRules::parse("required: upper; required: digit; allowed: lower, [-]; minlength: 20");
        let options = PassphraseOptions { words: 4, capitalize: true, number: true, ..PassphraseOptions::default() };
        for seed in 0..50 {
            let generated = passphrase(&options, Some(&rules), seed).unwrap();
            assert!(rules.is_satisfied_by(&generated.password) && generated.rules_met);
        }
        // Without capitals and digits, or too long for the site, no passphrase will do.
        assert_eq!(passphrase(&PassphraseOptions::default(), Some(&rules), 1).unwrap_err(), GeneratorError::RulesUnsatisfiable);
        assert_eq!(passphrase(&PassphraseOptions::default(), Some(&PasswordRules::parse("maxlength: 16")), 1).unwrap_err(), GeneratorError::RulesUnsatisfiable);
        assert_eq!(
            passphrase(&PassphraseOptions::default(), Some(&PasswordRules::parse("allowed: lower")), 1).unwrap_err(),
            GeneratorError::RulesUnsatisfiable
        );
    }

    #[test]
    fn a_failing_random_source_is_an_error() {
        let failed = generate(&GeneratorOptions::default(), None, &mut Scripted(vec![1, 2, 3])).unwrap_err();
        assert_eq!(failed, GeneratorError::Random(RandomUnavailable));
        let failed = generate(&GeneratorOptions::Passphrase(PassphraseOptions::default()), None, &mut Scripted(vec![])).unwrap_err();
        assert_eq!(failed, GeneratorError::Random(RandomUnavailable));
        assert_eq!(failed.to_string(), "the system's random number generator failed");
    }

    #[test]
    fn the_password_is_never_printed() {
        let generated = password(&PasswordOptions::default(), None, 1).unwrap();
        let printed = format!("{generated:?}");
        assert!(!printed.contains(generated.password.as_str()));
        assert!(printed.contains("<hidden>") && printed.contains(&generated.bits.to_string()));
    }

    #[test]
    fn the_system_generator_makes_different_passwords() {
        let mut random = OsRandom::new();
        let first = generate(&GeneratorOptions::default(), None, &mut random).unwrap();
        let second = generate(&GeneratorOptions::default(), None, &mut random).unwrap();
        assert_ne!(first.password, second.password);
        let dyn_random: &mut dyn RandomSource = &mut random;
        assert_eq!(generate(&GeneratorOptions::default(), None, dyn_random).unwrap().password.len(), 20);
    }

    #[test]
    fn characters_are_uniform() {
        // 2000 passwords of 128 digits: each digit about 25 600 times. Pearson's chi-squared, 9
        // degrees of freedom, below the 0.1 % critical value 27.88; the seeds are fixed.
        let mut counts: HashMap<char, u32> = HashMap::new();
        for seed in 0..2000 {
            for c in password(&PasswordOptions { length: 128, ..only(false, false, true, false) }, None, seed).unwrap().password.chars() {
                *counts.entry(c).or_default() += 1;
            }
        }
        let expected = 2000.0 * 128.0 / 10.0;
        let chi_squared: f64 = counts.values().map(|&count| (f64::from(count) - expected).powi(2) / expected).sum();
        assert_eq!(counts.len(), 10);
        assert!(chi_squared < 27.88, "chi-squared {chi_squared}: {counts:?}");
        // The words of passphrases too: 7776 of them, about 64 times each in 497 664 draws;
        // 7775 degrees of freedom, below the 0.1 % critical value 8166.
        let mut words: HashMap<String, u32> = HashMap::new();
        let mut random = Seeded(7);
        for _ in 0..(497_664 / 6) {
            let generated =
                generate(&GeneratorOptions::Passphrase(PassphraseOptions { separator: " ".to_owned(), ..PassphraseOptions::default() }), None, &mut random)
                    .unwrap();
            for word in generated.password.split(' ') {
                *words.entry(word.to_owned()).or_default() += 1;
            }
        }
        let chi_squared: f64 = words.values().map(|&count| (f64::from(count) - 64.0).powi(2) / 64.0).sum::<f64>() + 64.0 * (7776 - words.len()) as f64;
        assert!(chi_squared < 8166.0, "chi-squared {chi_squared} over {} words", words.len());
    }

    fn options_strategy() -> impl Strategy<Value = PasswordOptions> {
        (0u32..160, any::<[bool; 5]>(), proptest::collection::vec(proptest::char::range(' ', '~'), 0..12)).prop_map(|(length, flags, exclude)| {
            PasswordOptions {
                length,
                lower: flags[0],
                upper: flags[1],
                digits: flags[2],
                symbols: flags[3],
                avoid_ambiguous: flags[4],
                exclude: exclude.into_iter().collect(),
            }
        })
    }

    proptest! {
        #[test]
        fn passwords_keep_to_their_options(options in options_strategy(), seed in any::<u64>()) {
            let groups: Vec<String> = [(options.lower, LOWER), (options.upper, UPPER), (options.digits, DIGITS), (options.symbols, SYMBOLS)]
                .iter()
                .filter(|(chosen, _)| *chosen)
                .map(|(_, group)| group.chars().filter(|&c| !options.exclude.contains(c) && !(options.avoid_ambiguous && "Il1|O0".contains(c))).collect::<String>())
                .filter(|group| !group.is_empty())
                .collect();
            match password(&options, None, seed) {
                Ok(generated) => {
                    let text = generated.password.as_str();
                    prop_assert_eq!(text.chars().count() as u32, options.length.clamp(4, 128));
                    prop_assert!(text.chars().all(|c| groups.iter().any(|group| group.contains(c))), "{}", text);
                    if groups.len() <= text.len() {
                        prop_assert!(groups.iter().all(|group| text.chars().any(|c| group.contains(c))), "{}", text);
                    }
                }
                Err(error) => {
                    prop_assert_eq!(error, GeneratorError::NoCharacters);
                    prop_assert!(groups.is_empty());
                }
            }
        }

        #[test]
        fn passwords_keep_to_the_site_rules(index in 0usize..436, options in options_strategy(), seed in any::<u64>()) {
            let quirks = crate::rules::bundled_quirks(crate::data::PASSWORD_RULES).unwrap();
            let mut domains: Vec<&String> = quirks.keys().collect();
            domains.sort();
            let (rules, _) = &quirks[domains[index]];
            match password(&options, Some(rules), seed) {
                Ok(generated) => {
                    let text = generated.password.as_str();
                    prop_assert!(rules.is_satisfied_by(text), "{}: {}", domains[index], text);
                    prop_assert!(!text.chars().any(|c| options.exclude.contains(c) || (options.avoid_ambiguous && "Il1|O0".contains(c))), "{}", text);
                    prop_assert!(generated.rules_met);
                }
                // Only what the user excluded can stand in the way of the bundled rules.
                Err(error) => {
                    prop_assert_eq!(error, GeneratorError::RulesUnsatisfiable);
                    prop_assert!(!options.exclude.is_empty() || options.avoid_ambiguous);
                }
            }
        }
    }
}
