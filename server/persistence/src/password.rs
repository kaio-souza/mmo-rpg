//! argon2id with the crate defaults (OWASP recommendation). Both functions
//! are slow on purpose: callers run them on a blocking pool.

use crate::RepoError;
use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

pub fn hash_password(password: &str) -> Result<String, RepoError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| RepoError::Password(e.to_string()))
}

/// `false` also for a hash that does not parse.
pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}
