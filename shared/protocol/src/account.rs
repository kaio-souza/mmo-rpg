//! Account and session types (phase 2). Secrets never show in
//! `Debug`, so they cannot leak into a log by accident.

use base::{BoundedString, CharacterId, CharacterName, Locale, Look};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use zeroize::Zeroize;

/// 8 to 128 characters, no control characters. Wiped from memory when
/// dropped.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Password(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidPassword;

impl fmt::Display for InvalidPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "password must be {} to {} characters without control characters",
            Password::MIN_CHARS,
            Password::MAX_CHARS
        )
    }
}

impl std::error::Error for InvalidPassword {}

impl Password {
    pub const MIN_CHARS: usize = 8;
    pub const MAX_CHARS: usize = 128;

    pub fn expose(&self) -> &str {
        &self.0
    }

    /// The password may not contain the account name (any case).
    pub fn contains_name(&self, account: &str) -> bool {
        !account.is_empty() && self.0.to_lowercase().contains(&account.to_lowercase())
    }
}

impl TryFrom<String> for Password {
    type Error = InvalidPassword;

    fn try_from(mut s: String) -> Result<Self, Self::Error> {
        let ok = BoundedString::<128>::try_from(s.as_str()).is_ok() && s.chars().count() >= Self::MIN_CHARS;
        if !ok {
            s.zeroize();
            return Err(InvalidPassword);
        }
        Ok(Self(s))
    }
}

impl From<Password> for String {
    fn from(mut p: Password) -> String {
        std::mem::take(&mut p.0)
    }
}

impl Drop for Password {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(***)")
    }
}

/// Random session token. The server keeps only its hash.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionToken(pub [u8; 32]);

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(***)")
    }
}

/// Long-lived token of a device that logged in to an account:
/// with it, the account's own wait after wrong passwords does not apply.
/// The server keeps only its hash.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceToken(pub [u8; 32]);

impl fmt::Debug for DeviceToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceToken(***)")
    }
}

/// Proof of work for `Login` and `Register`: a nonce whose
/// hash with the connection's seed and the account starts with `bits`
/// zero bits. `bits` 0 accepts any nonce.
pub fn proof_ok(seed: &[u8; 16], account: &str, nonce: u64, bits: u8) -> bool {
    leading_zero_bits(&proof_hash(seed, account, nonce)) >= u32::from(bits)
}

/// The first nonce that passes `proof_ok`. About 2^bits hashes.
pub fn solve_proof(seed: &[u8; 16], account: &str, bits: u8) -> u64 {
    (0..u64::MAX)
        .find(|n| proof_ok(seed, account, *n, bits))
        .unwrap_or(0)
}

fn proof_hash(seed: &[u8; 16], account: &str, nonce: u64) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"mmo-kit.proof");
    h.update(seed);
    h.update(account.as_bytes());
    h.update(nonce.to_le_bytes());
    h.finalize().into()
}

fn leading_zero_bits(hash: &[u8; 32]) -> u32 {
    let mut n = 0;
    for b in hash {
        if *b == 0 {
            n += 8;
        } else {
            return n + b.leading_zeros();
        }
    }
    n
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterSummary {
    pub id: CharacterId,
    pub name: CharacterName,
    /// Phase 5 (protocol 6): for the selection screen.
    pub level: u8,
    pub look: Look,
}

/// One preference per message, so new ones are new variants (append only).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preference {
    Locale(#[serde(with = "crate::wire_locale")] Locale),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_rules_and_debug() {
        assert!(Password::try_from("short".to_owned()).is_err());
        assert!(Password::try_from("x".repeat(129)).is_err());
        assert!(Password::try_from("tab\there!".to_owned()).is_err());
        let p = Password::try_from("correct horse".to_owned()).unwrap();
        assert_eq!(p.expose(), "correct horse");
        assert_eq!(format!("{p:?}"), "Password(***)");
        assert_eq!(format!("{:?}", SessionToken([7; 32])), "SessionToken(***)");
        assert_eq!(format!("{:?}", DeviceToken([7; 32])), "DeviceToken(***)");
        assert!(p.contains_name("Horse"));
        assert!(!p.contains_name("kai"));
        assert_eq!(String::from(p), "correct horse");
    }

    #[test]
    fn proofs_bind_seed_account_and_bits() {
        let seed = [3u8; 16];
        assert!(proof_ok(&seed, "kai", 12345, 0));
        let n = solve_proof(&seed, "kai", 10);
        assert!(proof_ok(&seed, "kai", n, 10));
        // The same nonce for another account or seed almost never passes.
        let other = (10..18u8).filter(|i| proof_ok(&[*i; 16], "kai", n, 10)).count()
            + usize::from(proof_ok(&seed, "ana", n, 10));
        assert!(other <= 1, "{other}");
        assert_eq!(leading_zero_bits(&[0; 32]), 256);
    }
}
