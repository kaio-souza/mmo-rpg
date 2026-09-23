//! Strings validated at the boundary. Whoever receives one does not revalidate.

use std::fmt;
use std::ops::Deref;

/// A string of at most `N` characters (Unicode scalar values, not bytes),
/// without control characters. Built only through `TryFrom`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct BoundedString<const N: usize>(String);

/// Chat message text.
pub type ChatText = BoundedString<200>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedStringError {
    TooLong {
        max: usize,
        got: usize,
    },
    ControlChar,
    /// Only for `CharacterName`.
    InvalidName,
    /// Only for `AccountName`.
    InvalidAccount,
}

impl fmt::Display for BoundedStringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { max, got } => write!(f, "text too long: {got} > {max} characters"),
            Self::ControlChar => f.write_str("text contains control characters"),
            Self::InvalidName => f.write_str("name must be 3 to 16 characters of [A-Za-z0-9_]"),
            Self::InvalidAccount => f.write_str("account must be 3 to 24 characters of [a-z0-9_]"),
        }
    }
}

impl std::error::Error for BoundedStringError {}

impl<const N: usize> BoundedString<N> {
    pub const MAX_CHARS: usize = N;

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl<const N: usize> TryFrom<String> for BoundedString<N> {
    type Error = BoundedStringError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let got = s.chars().count();
        if got > N {
            return Err(BoundedStringError::TooLong { max: N, got });
        }
        if s.chars().any(char::is_control) {
            return Err(BoundedStringError::ControlChar);
        }
        Ok(Self(s))
    }
}

impl<const N: usize> TryFrom<&str> for BoundedString<N> {
    type Error = BoundedStringError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_owned())
    }
}

impl<const N: usize> From<BoundedString<N>> for String {
    fn from(s: BoundedString<N>) -> String {
        s.0
    }
}

impl<const N: usize> Deref for BoundedString<N> {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl<const N: usize> AsRef<str> for BoundedString<N> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<const N: usize> fmt::Display for BoundedString<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<const N: usize> fmt::Debug for BoundedString<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

/// Character name: 3 to 16 characters of `[A-Za-z0-9_]`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct CharacterName(BoundedString<16>);

impl CharacterName {
    pub const MIN_CHARS: usize = 3;
    pub const MAX_CHARS: usize = 16;

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<String> for CharacterName {
    type Error = BoundedStringError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let inner = BoundedString::<16>::try_from(s)?;
        let ok =
            inner.len() >= Self::MIN_CHARS && inner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        if !ok {
            return Err(BoundedStringError::InvalidName);
        }
        Ok(Self(inner))
    }
}

impl TryFrom<&str> for CharacterName {
    type Error = BoundedStringError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_owned())
    }
}

impl From<CharacterName> for String {
    fn from(n: CharacterName) -> String {
        n.0.into_string()
    }
}

impl Deref for CharacterName {
    type Target = str;
    fn deref(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for CharacterName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for CharacterName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

/// Account name: 3 to 24 characters of `[a-z0-9_]`. Upper case letters are
/// turned into lower case, so `Kai` and `kai` are the same account.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct AccountName(BoundedString<24>);

impl AccountName {
    pub const MIN_CHARS: usize = 3;
    pub const MAX_CHARS: usize = 24;

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<String> for AccountName {
    type Error = BoundedStringError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let inner = BoundedString::<24>::try_from(s.to_ascii_lowercase())
            .map_err(|_| BoundedStringError::InvalidAccount)?;
        let ok = inner.len() >= Self::MIN_CHARS
            && inner
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !ok {
            return Err(BoundedStringError::InvalidAccount);
        }
        Ok(Self(inner))
    }
}

impl TryFrom<&str> for AccountName {
    type Error = BoundedStringError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_owned())
    }
}

impl From<AccountName> for String {
    fn from(n: AccountName) -> String {
        n.0.into_string()
    }
}

impl fmt::Display for AccountName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for AccountName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_chars_not_bytes() {
        let s = "あ".repeat(200);
        assert!(ChatText::try_from(s.as_str()).is_ok());
        let s = "あ".repeat(201);
        assert_eq!(
            ChatText::try_from(s).unwrap_err(),
            BoundedStringError::TooLong { max: 200, got: 201 }
        );
    }

    #[test]
    fn rejects_control_chars() {
        assert_eq!(
            ChatText::try_from("a\nb").unwrap_err(),
            BoundedStringError::ControlChar
        );
    }

    #[test]
    fn character_name() {
        assert!(CharacterName::try_from("Kai_99").is_ok());
        for bad in ["ab", "a".repeat(17).as_str(), "has space", "ação"] {
            assert!(CharacterName::try_from(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn account_name() {
        assert_eq!(AccountName::try_from("Kai_99").unwrap().as_str(), "kai_99");
        for bad in ["ab", "a".repeat(25).as_str(), "has space", "ação", "a-b"] {
            assert!(AccountName::try_from(bad).is_err(), "{bad}");
        }
    }
}
