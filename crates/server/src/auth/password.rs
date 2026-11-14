//! Password hashing with Argon2id.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use thiserror::Error;

/// Shortest password accepted; the panel is reachable from the child's PC.
pub(crate) const MIN_PASSWORD_LENGTH: usize = 8;

/// A password that cannot be set or a hash that cannot be produced.
#[derive(Debug, Error)]
pub(crate) enum PasswordError {
    /// The password is shorter than [`MIN_PASSWORD_LENGTH`] characters.
    #[error("password must have at least {MIN_PASSWORD_LENGTH} characters")]
    TooShort,
    /// Hashing failed.
    #[error("hashing the password: {0}")]
    Hash(argon2::password_hash::Error),
}

/// Hashes `password` in PHC format with a random salt.
pub(crate) fn hash_password(password: &str) -> Result<String, PasswordError> {
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(PasswordError::TooShort);
    }
    let hash: PasswordHash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(PasswordError::Hash)?;
    Ok(hash.to_string())
}

/// Whether `password` matches `hash`; a malformed hash matches nothing.
pub(crate) fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_password_verifies() {
        let hash = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &hash));
        assert!(!verify_password("wrong horse", &hash));
    }

    #[test]
    fn short_password_is_rejected() {
        assert!(matches!(
            hash_password("short"),
            Err(PasswordError::TooShort)
        ));
    }

    #[test]
    fn malformed_hash_matches_nothing() {
        assert!(!verify_password("anything", "not a hash"));
    }
}
