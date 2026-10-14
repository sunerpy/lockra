//! Sets of characters, as password rules and the generator speak of them: ASCII printable ones,
//! one bit each from the space to `~`, or every character there is.

use crate::random::{RandomSource, RandomUnavailable};

/// A set of ASCII printable characters, or of every character (`unicode`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct CharacterSet {
    ascii: u128,
    unicode: bool,
}

impl CharacterSet {
    /// No character.
    pub(crate) const EMPTY: Self = Self { ascii: 0, unicode: false };
    /// The 95 ASCII printable characters, the space included.
    pub(crate) const ASCII_PRINTABLE: Self = Self::range(' ', '~');
    /// Every character (whose ASCII printable members are all 95).
    pub(crate) const UNICODE: Self = Self { ascii: Self::ASCII_PRINTABLE.ascii, unicode: true };
    /// `a` to `z`.
    pub(crate) const LOWER: Self = Self::range('a', 'z');
    /// `A` to `Z`.
    pub(crate) const UPPER: Self = Self::range('A', 'Z');
    /// `0` to `9`.
    pub(crate) const DIGIT: Self = Self::range('0', '9');
    /// The 33 ASCII printable characters that are no letter and no digit, the space included.
    pub(crate) const SPECIAL: Self = Self { ascii: Self::ASCII_PRINTABLE.ascii & !(Self::LOWER.ascii | Self::UPPER.ascii | Self::DIGIT.ascii), unicode: false };
    /// The 32 ASCII punctuation characters: [`Self::SPECIAL`] without the space.
    pub(crate) const PUNCTUATION: Self = Self { ascii: Self::SPECIAL.ascii & !Self::range(' ', ' ').ascii, unicode: false };

    /// `first` to `last`, both ASCII printable.
    const fn range(first: char, last: char) -> Self {
        let mut ascii = 0;
        let mut c = first as u32;
        while c <= last as u32 {
            ascii |= 1 << (c - ' ' as u32);
            c += 1;
        }
        Self { ascii, unicode: false }
    }

    /// The ASCII printable characters among `characters`; the others are dropped.
    pub(crate) fn of(characters: impl IntoIterator<Item = char>) -> Self {
        let ascii = characters.into_iter().filter(|&c| is_ascii_printable(c)).fold(0, |ascii, c| ascii | bit(c));
        Self { ascii, unicode: false }
    }

    /// Whether `c` is in the set.
    pub(crate) fn contains(self, c: char) -> bool {
        self.unicode || (is_ascii_printable(c) && self.ascii & bit(c) != 0)
    }

    /// Whether the set has no character.
    pub(crate) fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    /// The characters of either set.
    pub(crate) fn union(self, other: Self) -> Self {
        Self { ascii: self.ascii | other.ascii, unicode: self.unicode || other.unicode }
    }

    /// The ASCII printable members; for `unicode`, all of them.
    pub(crate) fn printable(self) -> Self {
        if self.unicode { Self::ASCII_PRINTABLE } else { self }
    }

    /// The characters of both sets.
    pub(crate) fn intersection(self, other: Self) -> Self {
        match (self.unicode, other.unicode) {
            (true, true) => self,
            (true, false) => other,
            (false, true) => self,
            (false, false) => Self { ascii: self.ascii & other.ascii, unicode: false },
        }
    }

    /// The ASCII printable members without those of `other`.
    pub(crate) fn without(self, other: Self) -> Self {
        Self { ascii: self.printable().ascii & !other.printable().ascii, unicode: false }
    }

    /// Whether every member is also one of `other`'s.
    pub(crate) fn is_subset(self, other: Self) -> bool {
        other.unicode || (!self.unicode && self.ascii & !other.ascii == 0)
    }

    /// How many ASCII printable members there are (95 for `unicode`).
    pub(crate) fn len(self) -> u32 {
        self.printable().ascii.count_ones()
    }

    /// The ASCII printable members, in ASCII order.
    pub(crate) fn chars(self) -> impl Iterator<Item = char> {
        let ascii = self.printable().ascii;
        (' '..='~').filter(move |&c| ascii & bit(c) != 0)
    }

    /// A member drawn uniformly at random from the ASCII printable ones; `None` for an empty set.
    pub(crate) fn pick(self, random: &mut (impl RandomSource + ?Sized)) -> Result<Option<char>, RandomUnavailable> {
        let index = random.below(self.len())?;
        Ok(self.chars().nth(index as usize))
    }
}

impl std::fmt::Debug for CharacterSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.unicode { f.write_str("unicode") } else { write!(f, "[{}]", self.chars().collect::<String>()) }
    }
}

/// Whether `c` is one of the 95 ASCII printable characters, the space included.
pub(crate) fn is_ascii_printable(c: char) -> bool {
    (' '..='~').contains(&c)
}

/// The bit of an ASCII printable character.
fn bit(c: char) -> u128 {
    1 << (u32::from(c) - u32::from(' '))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_sets_have_their_characters() {
        assert_eq!((CharacterSet::ASCII_PRINTABLE.len(), CharacterSet::SPECIAL.len(), CharacterSet::PUNCTUATION.len()), (95, 33, 32));
        assert_eq!(format!("{:?}", CharacterSet::DIGIT), "[0123456789]");
        assert_eq!(format!("{:?}", CharacterSet::UNICODE), "unicode");
        assert!(CharacterSet::SPECIAL.contains(' ') && !CharacterSet::PUNCTUATION.contains(' '));
        assert!(CharacterSet::UNICODE.contains('密') && !CharacterSet::ASCII_PRINTABLE.contains('密'));
        assert_eq!(CharacterSet::of("bac\u{0}é".chars()), CharacterSet::of(['a', 'b', 'c']));
        assert!(CharacterSet::EMPTY.is_empty() && !CharacterSet::of(['a']).is_empty());
        assert_eq!(CharacterSet::range('a', 'c'), CharacterSet::of(['c', 'b', 'a']));
        assert_eq!(CharacterSet::range(' ', '~'), CharacterSet::ASCII_PRINTABLE);
    }

    #[test]
    fn unicode_contains_every_set() {
        let letters = CharacterSet::LOWER.union(CharacterSet::UPPER);
        assert_eq!(CharacterSet::UNICODE.intersection(letters), letters);
        assert_eq!(letters.intersection(CharacterSet::UNICODE), letters);
        assert_eq!(CharacterSet::UNICODE.intersection(CharacterSet::UNICODE), CharacterSet::UNICODE);
        assert_eq!(letters.union(CharacterSet::UNICODE), CharacterSet::UNICODE);
        assert!(letters.is_subset(CharacterSet::UNICODE) && !CharacterSet::UNICODE.is_subset(letters));
        assert!(CharacterSet::LOWER.is_subset(letters) && !letters.is_subset(CharacterSet::LOWER));
        // Taking characters away leaves ASCII ones only.
        assert_eq!(CharacterSet::UNICODE.without(CharacterSet::SPECIAL), CharacterSet::LOWER.union(CharacterSet::UPPER).union(CharacterSet::DIGIT));
        assert_eq!(CharacterSet::UNICODE.printable(), CharacterSet::ASCII_PRINTABLE);
    }
}
