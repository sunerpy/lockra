//! The EFF's large wordlist for passphrases: 7776 words, one for each roll of five dice, chosen
//! to be easy to type and remember (<https://www.eff.org/dice>).

use std::sync::LazyLock;

use crate::data;

/// The words, in the list's order, parsed on first use.
static WORDS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| data::EFF_LARGE_WORDLIST.lines().filter_map(|line| line.split_once('\t')).map(|(_, word)| word).collect());

/// The words of the list.
pub(crate) fn words() -> &'static [&'static str] {
    &WORDS
}

/// The length of the longest word, to size a passphrase before it is written.
pub(crate) const LONGEST_WORD: usize = 9;

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn the_list_has_one_word_per_roll_of_five_dice() {
        let words = words();
        assert_eq!(words.len(), 7776);
        assert_eq!(words.iter().collect::<HashSet<_>>().len(), 7776);
        assert_eq!((words[0], words[7775]), ("abacus", "zoom"));
        // Every line is five dice (1 to 6), a tab and the word, in the order of the rolls.
        let rolls: Vec<&str> = data::EFF_LARGE_WORDLIST.lines().map(|line| line.split_once('\t').unwrap().0).collect();
        assert!(rolls.iter().all(|roll| roll.len() == 5 && roll.bytes().all(|die| (b'1'..=b'6').contains(&die))));
        assert!(rolls.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn words_are_short_lowercase_ascii() {
        for word in words() {
            assert!((3..=LONGEST_WORD).contains(&word.len()), "{word}");
            assert!(word.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'-'), "{word}");
            assert!(word.as_bytes()[0].is_ascii_lowercase(), "{word}");
        }
        assert_eq!(words().iter().map(|word| word.len()).max(), Some(LONGEST_WORD));
        // Four words carry a hyphen of their own.
        assert_eq!(words().iter().filter(|word| word.contains('-')).count(), 4);
    }
}
