//! How strong a password is, as zxcvbn estimates it: a score from 0 to 4 and, for weak ones, a
//! warning as a stable code that the interface translates (zxcvbn's English text is not used).

use serde::{Deserialize, Serialize};
use zxcvbn::feedback::Warning;

/// The most characters of a password that are analysed: zxcvbn's time grows quickly with the
/// length, and the first hundred characters decide the score anyway. A user input longer than
/// this cannot be found in what is analysed, and is left out.
pub const STRENGTH_MAX_CHARS: usize = 100;

/// A strength estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strength {
    /// 0 (guessed within a thousand tries) to 4 (more than 10^10 tries); below 3 is weak.
    pub score: u8,
    /// What makes the password weak, when zxcvbn can tell (only for scores up to 2).
    pub warning: Option<StrengthWarning>,
}

/// What makes a password weak; serialized as its snake_case code (`top_ten`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrengthWarning {
    /// A straight row of keys (`poiuy`).
    StraightRow,
    /// A short keyboard pattern (`asdcvb`).
    KeyboardPattern,
    /// A character repeated (`aaaaaaaa`).
    RepeatedCharacters,
    /// A pattern repeated (`abcabcabc`).
    RepeatedPattern,
    /// One of the ten most common passwords.
    TopTen,
    /// One of the hundred most common passwords.
    TopHundred,
    /// A very common password.
    CommonPassword,
    /// Close to a common password (`Password1!`).
    SimilarToCommon,
    /// A sequence (`abcdefgh`, `6543`).
    Sequence,
    /// A recent year.
    RecentYear,
    /// A single word.
    SingleWord,
    /// A date.
    Date,
    /// A name or surname by itself.
    Name,
    /// A common name and surname.
    CommonName,
}

impl StrengthWarning {
    /// The code, as serialized.
    pub fn code(self) -> &'static str {
        match self {
            Self::StraightRow => "straight_row",
            Self::KeyboardPattern => "keyboard_pattern",
            Self::RepeatedCharacters => "repeated_characters",
            Self::RepeatedPattern => "repeated_pattern",
            Self::TopTen => "top_ten",
            Self::TopHundred => "top_hundred",
            Self::CommonPassword => "common_password",
            Self::SimilarToCommon => "similar_to_common",
            Self::Sequence => "sequence",
            Self::RecentYear => "recent_year",
            Self::SingleWord => "single_word",
            Self::Date => "date",
            Self::Name => "name",
            Self::CommonName => "common_name",
        }
    }
}

impl From<Warning> for StrengthWarning {
    fn from(warning: Warning) -> Self {
        // Exhaustive: a warning added in a later zxcvbn fails to build until it has a code.
        match warning {
            Warning::StraightRowsOfKeysAreEasyToGuess => Self::StraightRow,
            Warning::ShortKeyboardPatternsAreEasyToGuess => Self::KeyboardPattern,
            Warning::RepeatsLikeAaaAreEasyToGuess => Self::RepeatedCharacters,
            Warning::RepeatsLikeAbcAbcAreOnlySlightlyHarderToGuess => Self::RepeatedPattern,
            Warning::ThisIsATop10Password => Self::TopTen,
            Warning::ThisIsATop100Password => Self::TopHundred,
            Warning::ThisIsACommonPassword => Self::CommonPassword,
            Warning::ThisIsSimilarToACommonlyUsedPassword => Self::SimilarToCommon,
            Warning::SequencesLikeAbcAreEasyToGuess => Self::Sequence,
            Warning::RecentYearsAreEasyToGuess => Self::RecentYear,
            Warning::AWordByItselfIsEasyToGuess => Self::SingleWord,
            Warning::DatesAreOftenEasyToGuess => Self::Date,
            Warning::NamesAndSurnamesByThemselvesAreEasyToGuess => Self::Name,
            Warning::CommonNamesAndSurnamesAreEasyToGuess => Self::CommonName,
        }
    }
}

/// Estimate the strength of `password`. `user_inputs` are words the password should not lean on,
/// such as the account's username, email address or site; a password made of them scores low.
pub fn strength(password: &str, user_inputs: &[&str]) -> Strength {
    let end = password.char_indices().nth(STRENGTH_MAX_CHARS).map_or(password.len(), |(end, _)| end);
    let inputs: Vec<&str> = user_inputs.iter().copied().filter(|input| input.chars().nth(STRENGTH_MAX_CHARS).is_none()).collect();
    let estimate = zxcvbn::zxcvbn(&password[..end], &inputs);
    Strength { score: u8::from(estimate.score()), warning: estimate.feedback().and_then(|feedback| feedback.warning()).map(StrengthWarning::from) }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn weak_passwords_score_low_with_a_reason() {
        use StrengthWarning::*;
        let cases = [
            ("poiuy", StraightRow),
            ("asdcvb", KeyboardPattern),
            ("aaaaaaaa", RepeatedCharacters),
            ("abcabcabc", RepeatedPattern),
            ("password", TopTen),
            ("qwertyuiop", TopHundred),
            ("monkey1", CommonPassword),
            ("Password1!", SimilarToCommon),
            ("abcdefgh", Sequence),
            ("2019", RecentYear),
            ("december", SingleWord),
            ("1987-05-12", Date),
            ("john", Name),
            ("johnsmith", CommonName),
        ];
        for (password, warning) in cases {
            let estimate = strength(password, &[]);
            assert!(estimate.score <= 2, "{password}: {estimate:?}");
            assert_eq!(estimate.warning, Some(warning), "{password}");
        }
    }

    #[test]
    fn strong_passwords_score_high_without_a_warning() {
        for password in ["correct horse battery staple", "k8#Vq!2zLp@9", "Tr0ub4dor&3"] {
            assert_eq!(strength(password, &[]), Strength { score: 4, warning: None }, "{password}");
        }
        assert_eq!(strength("", &[]), Strength { score: 0, warning: None });
    }

    #[test]
    fn user_inputs_make_a_password_weak() {
        assert_eq!(strength("lockra-alice-2026", &[]).score, 4);
        assert_eq!(strength("lockra-alice-2026", &["lockra-alice-2026"]).score, 0);
        // An input is a whole word: one longer than the analysed part cannot be in it.
        let long = format!("lockra-alice-2026{}", "x".repeat(10_000));
        assert_eq!(strength("lockra-alice-2026", &[&long, "unrelated"]).score, 4);
        let longest = "y".repeat(STRENGTH_MAX_CHARS);
        assert_eq!(strength(&longest, &[&longest]).score, 0);
    }

    #[test]
    fn warnings_have_stable_codes() {
        use StrengthWarning::*;
        let all = [
            StraightRow,
            KeyboardPattern,
            RepeatedCharacters,
            RepeatedPattern,
            TopTen,
            TopHundred,
            CommonPassword,
            SimilarToCommon,
            Sequence,
            RecentYear,
            SingleWord,
            Date,
            Name,
            CommonName,
        ];
        let codes: Vec<&str> = all.iter().map(|warning| warning.code()).collect();
        assert_eq!(
            codes,
            [
                "straight_row",
                "keyboard_pattern",
                "repeated_characters",
                "repeated_pattern",
                "top_ten",
                "top_hundred",
                "common_password",
                "similar_to_common",
                "sequence",
                "recent_year",
                "single_word",
                "date",
                "name",
                "common_name"
            ]
        );
        for warning in all {
            assert_eq!(serde_json::to_value(warning).unwrap(), serde_json::Value::from(warning.code()));
        }
        assert_eq!(serde_json::to_string(&strength("password", &[])).unwrap(), r#"{"score":0,"warning":"top_ten"}"#);
        assert_eq!(serde_json::to_string(&strength("k8#Vq!2zLp@9", &[])).unwrap(), r#"{"score":4,"warning":null}"#);
    }

    #[test]
    fn long_input_is_cut_and_quick() {
        let long: String = "Ab1!xQ9#".chars().cycle().take(10_000).collect();
        let started = Instant::now();
        let estimate = strength(&long, &[&long]);
        let elapsed = started.elapsed();
        // The password cut to its first hundred characters, the input too long to be in them.
        let prefix: String = long.chars().take(STRENGTH_MAX_CHARS).collect();
        assert_eq!(estimate, strength(&prefix, &[]));
        // A debug build under coverage takes tens of milliseconds; the whole input would take far longer.
        assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
        // Characters, not bytes: a password of wide characters is cut on a character boundary.
        let wide = "密".repeat(STRENGTH_MAX_CHARS + 1);
        assert_eq!(strength(&wide, &[]), strength(&"密".repeat(STRENGTH_MAX_CHARS), &[]));
    }
}
