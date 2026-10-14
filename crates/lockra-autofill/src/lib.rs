//! Autofill's pure logic, for the core and the browser channel: no I/O, no network, no clock.
//!
//! - **Matching** ([`best_matches`]): which items' saved URLs match a page, keeping only the items
//!   at the closest level any of them reaches. [`WebUrl`] normalizes addresses and names their
//!   site, the registrable domain by the Public Suffix List (the `psl` crate);
//!   [`EquivalentDomains`] holds the domains that share a sign-in.
//! - **Password rules** ([`PasswordRules`]): the `passwordrules` language sites publish, and the
//!   bundled rules of sites that do not ([`bundled_password_rules`]).
//! - **Generator** ([`generate`]): random passwords and passphrases, held to a site's rules; the
//!   randomness comes through [`RandomSource`].
//! - **Strength** ([`strength`]): zxcvbn's score and a warning code for the interface to word.
//!
//! The third-party data, Apple's password-manager-resources lists and the EFF's large wordlist,
//! is compiled in byte for byte as published: `data/SOURCES` pins each file to its upstream
//! commit and SHA-256, and only `scripts/update-vendored-data.sh` changes them.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod address;
mod charset;
mod data;
mod equivalent;
mod generator;
mod matching;
mod parser;
mod random;
mod rules;
mod strength;
mod wordlist;

pub use address::WebUrl;
pub use equivalent::EquivalentDomains;
pub use generator::{
    Generated, GeneratorError, GeneratorOptions, PASSPHRASE_WORDS, PASSWORD_LENGTHS, PassphraseOptions, PasswordOptions, SEPARATOR_MAX_CHARS, generate,
};
pub use matching::{BestMatches, ItemMatch, MatchLevel, Matching, SavedUrl, UrlMatch, best_matches, match_url};
pub use random::{OsRandom, RandomSource, RandomUnavailable};
pub use rules::{PasswordRules, bundled_password_rules};
pub use strength::{STRENGTH_MAX_CHARS, Strength, StrengthWarning, strength};
