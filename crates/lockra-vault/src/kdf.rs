//! Argon2id (RFC 9106) from the master password to the password slot's key.

use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::b64;
use crate::container::VaultError;

/// The only KDF a format-1 header names.
pub(crate) const ARGON2ID: &str = "argon2id";
pub(crate) const SALT_LEN: usize = 16;

/// The most a file may ask for, so that a hostile backup cannot make the app allocate gigabytes
/// or spin for minutes before the password is even checked: four times the default memory.
const MAX_M_KIB: u32 = 256 * 1024;
const MAX_T: u32 = 16;
const MAX_P: u32 = 8;

/// How expensive the derivation is. Written into the header, so a file always opens with the
/// parameters it was written with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfCost {
    pub(crate) m_kib: u32,
    pub(crate) t: u32,
    pub(crate) p: u32,
}

impl KdfCost {
    /// 64 MiB, three passes, one lane: above OWASP's minimum for Argon2id (19 MiB, two passes) and
    /// still well under a second on a laptop.
    pub const DEFAULT: Self = Self { m_kib: 64 * 1024, t: 3, p: 1 };

    /// Argon2's minimum: for test suites only, never compiled into an application build.
    #[cfg(any(test, feature = "test-kdf"))]
    pub const FAST_INSECURE: Self = Self { m_kib: 8, t: 1, p: 1 };
}

/// The KDF entry of a header: algorithm, cost and salt. The sync keyring (lockra-sync) uses the
/// same entry for the password half of its key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub(crate) algorithm: String,
    pub(crate) m_kib: u32,
    pub(crate) t: u32,
    pub(crate) p: u32,
    #[serde(with = "b64::array")]
    pub(crate) salt: [u8; SALT_LEN],
}

impl KdfParams {
    /// `cost` with a fresh random salt.
    pub fn fresh(cost: KdfCost) -> Result<Self, VaultError> {
        let mut salt = [0u8; SALT_LEN];
        getrandom::fill(&mut salt).map_err(|_| VaultError::Random)?;
        Ok(Self { algorithm: ARGON2ID.to_owned(), m_kib: cost.m_kib, t: cost.t, p: cost.p, salt })
    }

    /// The 32-byte key for `password`. Parameters outside what Lockra writes count as a damaged file.
    pub fn derive(&self, password: &[u8]) -> Result<Zeroizing<[u8; 32]>, VaultError> {
        if self.algorithm != ARGON2ID || self.m_kib > MAX_M_KIB || self.t > MAX_T || self.p > MAX_P {
            return Err(VaultError::Corrupted);
        }
        let params = Params::new(self.m_kib, self.t, self.p, Some(32)).map_err(|_| VaultError::Corrupted)?;
        let mut key = Zeroizing::new([0u8; 32]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(password, &self.salt, key.as_mut()).map_err(|_| VaultError::Kdf)?;
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(salt: u8) -> KdfParams {
        KdfParams { algorithm: ARGON2ID.into(), m_kib: 8, t: 1, p: 1, salt: [salt; SALT_LEN] }
    }

    #[test]
    fn derivation_is_deterministic_and_salted() {
        let a = params(1).derive(b"pw").unwrap();
        assert_eq!(*a, *params(1).derive(b"pw").unwrap());
        assert_ne!(*a, *params(2).derive(b"pw").unwrap());
        assert_ne!(*a, *params(1).derive(b"pW").unwrap());
    }

    #[test]
    fn hostile_parameters_are_refused_before_any_work() {
        for mutate in [
            (|p: &mut KdfParams| p.algorithm = "argon2i".into()) as fn(&mut KdfParams),
            |p| p.m_kib = MAX_M_KIB + 1,
            |p| p.t = MAX_T + 1,
            |p| p.p = MAX_P + 1,
            |p| p.t = 0,
        ] {
            let mut p = params(1);
            mutate(&mut p);
            assert!(matches!(p.derive(b"pw"), Err(VaultError::Corrupted)), "{p:?}");
        }
    }

    #[test]
    fn fresh_salts_differ_and_keep_the_cost() {
        let a = KdfParams::fresh(KdfCost::DEFAULT).unwrap();
        let b = KdfParams::fresh(KdfCost::DEFAULT).unwrap();
        assert_ne!(a.salt, b.salt);
        let d = KdfCost::DEFAULT;
        assert_eq!((a.algorithm.as_str(), a.m_kib, a.t, a.p), (ARGON2ID, d.m_kib, d.t, d.p));
    }
}
