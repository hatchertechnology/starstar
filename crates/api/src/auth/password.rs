//! Password hashing (argon2id) and validation.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

use super::crypto::random_bytes;
use crate::error::ApiError;

type DummyHashes = HashMap<(u32, u32, u32), String>;

/// Argon2id with fixed cost parameters.
#[derive(Clone, Copy)]
pub struct Hasher {
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
}

impl Hasher {
    /// OWASP minimum for argon2id: 19 MiB, 2 passes, 1 lane.
    pub fn secure() -> Self {
        Self {
            m_cost: 19_456,
            t_cost: 2,
            p_cost: 1,
        }
    }

    /// Cheap parameters so tests stay fast. Never use outside tests.
    pub fn fast_for_tests() -> Self {
        Self {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        }
    }

    fn argon(&self) -> Argon2<'static> {
        let params = Params::new(self.m_cost, self.t_cost, self.p_cost, None)
            .expect("argon2 parameters are valid");
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
    }

    /// A PHC string for `password`.
    pub fn hash(&self, password: &str) -> String {
        let salt = SaltString::encode_b64(&random_bytes::<16>()).expect("16 bytes is a valid salt");
        self.argon()
            .hash_password(password.as_bytes(), &salt)
            .expect("argon2 hashing with valid parameters cannot fail")
            .to_string()
    }

    /// False for a wrong password or an unparseable hash.
    pub fn verify(&self, password: &str, phc: &str) -> bool {
        match PasswordHash::new(phc) {
            Ok(parsed) => self
                .argon()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok(),
            Err(_) => false,
        }
    }

    fn dummies() -> &'static Mutex<DummyHashes> {
        static DUMMIES: OnceLock<Mutex<DummyHashes>> = OnceLock::new();
        DUMMIES.get_or_init(Default::default)
    }

    fn dummy_key(&self) -> (u32, u32, u32) {
        (self.m_cost, self.t_cost, self.p_cost)
    }

    fn dummy_phc(&self) -> String {
        Self::dummies()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(self.dummy_key())
            .or_insert_with(|| self.hash("dummy password for timing"))
            .clone()
    }

    /// Computes the dummy hash now, so the first unknown-user sign-in is not
    /// slower than the rest. Call once at startup.
    pub fn prewarm(&self) {
        let _ = self.dummy_phc();
    }

    /// Spends the same time as a real verification, so a missing user is not
    /// distinguishable by timing.
    pub fn dummy_verify(&self, password: &str) {
        let phc = self.dummy_phc();
        let _ = self.verify(password, &phc);
    }
}

/// The longest password accepted, in characters. Longer input is refused
/// before any argon2 run.
pub const MAX_PASSWORD_CHARS: usize = 256;

/// Passwords are 8 to 256 characters.
pub fn validate_password(password: &str) -> Result<(), ApiError> {
    if (8..=MAX_PASSWORD_CHARS).contains(&password.chars().count()) {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "passwords must be 8 to 256 characters".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_round_trip() {
        let h = Hasher::fast_for_tests();
        let phc = h.hash("correct horse");
        assert!(phc.starts_with("$argon2id$"));
        assert!(h.verify("correct horse", &phc));
        assert!(!h.verify("wrong", &phc));
        assert!(!h.verify("x", "not a hash"));
        h.dummy_verify("anything");
    }

    #[test]
    fn prewarm_computes_the_dummy_hash_up_front() {
        // Cost parameters no other test uses, so nothing cached them already.
        let h = Hasher {
            m_cost: 9,
            t_cost: 1,
            p_cost: 1,
        };
        let cached = |h: &Hasher| {
            Hasher::dummies()
                .lock()
                .unwrap()
                .contains_key(&h.dummy_key())
        };
        assert!(!cached(&h));
        h.prewarm();
        assert!(cached(&h));
    }

    #[test]
    fn password_length_rules() {
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password(&"x".repeat(257)).is_err());
    }
}
