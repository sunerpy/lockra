//! Randomness for the generator, behind a trait so that tests can replay it: the operating
//! system's generator in the app, a seeded one in tests.

use zeroize::Zeroize;

/// The operating system's random number generator failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the system's random number generator failed")]
pub struct RandomUnavailable;

/// A source of uniformly distributed random bits.
pub trait RandomSource {
    /// 32 random bits.
    fn next_u32(&mut self) -> Result<u32, RandomUnavailable>;

    /// A number uniformly distributed in `0..n`; 0 when `n` is 0 or 1. Draws among the lowest
    /// `2^32 mod n` values, which would make the low results likelier, are drawn again.
    fn below(&mut self, n: u32) -> Result<u32, RandomUnavailable> {
        if n <= 1 {
            return Ok(0);
        }
        // 2^32 mod n: the values below it are the surplus that does not divide evenly into n.
        let surplus = n.wrapping_neg() % n;
        loop {
            let draw = self.next_u32()?;
            if draw >= surplus {
                return Ok(draw % n);
            }
        }
    }
}

/// The operating system's generator (`getrandom`), read 64 bytes at a time. Each 4 bytes are
/// wiped as soon as they are handed out, and the rest when the source is dropped.
pub struct OsRandom {
    buffer: [u8; 64],
    used: usize,
}

impl OsRandom {
    /// A source that reads the system's generator on first use.
    pub fn new() -> Self {
        Self { buffer: [0; 64], used: 64 }
    }
}

impl Default for OsRandom {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for OsRandom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OsRandom")
    }
}

impl RandomSource for OsRandom {
    fn next_u32(&mut self) -> Result<u32, RandomUnavailable> {
        if self.used + 4 > self.buffer.len() {
            getrandom::fill(&mut self.buffer).map_err(|_| RandomUnavailable)?;
            self.used = 0;
        }
        let bytes = &mut self.buffer[self.used..self.used + 4];
        let draw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        bytes.zeroize();
        self.used += 4;
        Ok(draw)
    }
}

impl Drop for OsRandom {
    fn drop(&mut self) {
        self.buffer.zeroize();
    }
}

/// Deterministic sources for the tests of this crate.
#[cfg(test)]
pub(crate) mod testing {
    use super::{RandomSource, RandomUnavailable};

    /// SplitMix64 from a seed: the same seed, the same numbers.
    pub(crate) struct Seeded(pub(crate) u64);

    impl RandomSource for Seeded {
        fn next_u32(&mut self) -> Result<u32, RandomUnavailable> {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            Ok(((z ^ (z >> 31)) >> 32) as u32)
        }
    }

    /// The given draws, in order, then a failure.
    pub(crate) struct Scripted(pub(crate) Vec<u32>);

    impl RandomSource for Scripted {
        fn next_u32(&mut self) -> Result<u32, RandomUnavailable> {
            if self.0.is_empty() { Err(RandomUnavailable) } else { Ok(self.0.remove(0)) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Scripted, Seeded};
    use super::*;

    #[test]
    fn below_stays_in_range() {
        let mut random = Seeded(1);
        for n in [2, 3, 7, 10, 26, 95, 7776, 1 << 31, u32::MAX] {
            for _ in 0..200 {
                assert!(random.below(n).unwrap() < n);
            }
        }
        assert_eq!(random.below(1).unwrap(), 0);
        assert_eq!(random.below(0).unwrap(), 0);
    }

    #[test]
    fn below_draws_again_instead_of_favouring_low_numbers() {
        // 2^32 mod 3 is 1: a draw of 0 would make 0 likelier than 1 and 2.
        assert_eq!(Scripted(vec![0, 5]).below(3), Ok(2));
        assert_eq!(Scripted(vec![1, 5]).below(3), Ok(1));
        // 2^32 mod 10 is 6: the six lowest draws are thrown away.
        assert_eq!(Scripted(vec![0, 1, 2, 3, 4, 5, 16]).below(10), Ok(6));
        assert_eq!(Scripted(vec![u32::MAX]).below(10), Ok(5));
        // A power of two wastes nothing.
        assert_eq!(Scripted(vec![0]).below(8), Ok(0));
        assert_eq!(Scripted(vec![]).below(3), Err(RandomUnavailable));
    }

    #[test]
    fn below_is_uniform() {
        // Pearson's chi-squared over 100 000 draws of 0..10: 9 degrees of freedom, and 27.88 is
        // the 0.1 % critical value. The seed is fixed, so the test is deterministic.
        let mut random = Seeded(0x5eed);
        let mut counts = [0u32; 10];
        for _ in 0..100_000 {
            counts[random.below(10).unwrap() as usize] += 1;
        }
        let chi_squared: f64 = counts.iter().map(|&count| (f64::from(count) - 10_000.0).powi(2) / 10_000.0).sum();
        assert!(chi_squared < 27.88, "chi-squared {chi_squared}: {counts:?}");
    }

    #[test]
    fn the_system_generator_works() {
        let mut random = OsRandom::new();
        // More than one buffer's worth, so it refills.
        let draws: Vec<u32> = (0..100).map(|_| random.next_u32().unwrap()).collect();
        assert!(draws.iter().any(|&draw| draw != draws[0]), "100 equal draws from the system generator");
        assert!((0..100).all(|_| random.below(6).unwrap() < 6));
        assert_eq!(format!("{random:?}"), "OsRandom");
        // Handed-out bytes are wiped at once.
        assert!(random.buffer[..random.used].iter().all(|&byte| byte == 0));
        assert_eq!(format!("{:?}", OsRandom::default()), "OsRandom");
    }
}
