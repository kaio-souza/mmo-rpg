//! A locale is its code (`en`, `pt-BR`): which ones the game has is data,
//! `content/text/locales.ron`. Plural rules, number formatting
//! and text packs are in `i18n`.

use std::fmt;
use std::str::FromStr;

/// Longest code kept (`zh-Hant-TW` fits).
const MAX_LEN: usize = 15;

/// A locale code: 2 to 15 ASCII letters, digits and `-`, starting with a
/// letter. `Copy`, so it travels by value like the closed list it replaced.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Locale {
    bytes: [u8; MAX_LEN],
    len: u8,
}

impl Locale {
    /// English: the source language and the fallback.
    pub const EN: Locale = Locale::from_static("en");
    pub const SOURCE: Locale = Locale::EN;

    /// A code known at compile time; a bad one fails the build.
    pub const fn from_static(code: &str) -> Locale {
        match Locale::parse_bytes(code.as_bytes()) {
            Some(l) => l,
            None => panic!("bad locale code"),
        }
    }

    // A `const fn` cannot use `get`; every index is below `b.len()`, which
    // is checked to be at most `MAX_LEN` first.
    #[allow(clippy::indexing_slicing)]
    const fn parse_bytes(b: &[u8]) -> Option<Locale> {
        if b.len() < 2 || b.len() > MAX_LEN || !b[0].is_ascii_alphabetic() {
            return None;
        }
        let mut bytes = [0u8; MAX_LEN];
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if !(c.is_ascii_alphanumeric() || c == b'-') {
                return None;
            }
            bytes[i] = c;
            i += 1;
        }
        Some(Locale {
            bytes,
            len: b.len() as u8,
        })
    }

    /// Code used in folder names, `Hello` and preferences (`pt-BR`).
    pub fn code(&self) -> &str {
        let b = self.bytes.get(..usize::from(self.len)).unwrap_or_default();
        std::str::from_utf8(b).unwrap_or_default()
    }

    /// The language part of the code (`pt` of `pt-BR`).
    pub fn language(&self) -> &str {
        self.code().split('-').next().unwrap_or_default()
    }
}

impl Default for Locale {
    fn default() -> Self {
        Locale::EN
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl fmt::Debug for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Locale({})", self.code())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseLocaleError(pub String);

impl fmt::Display for ParseLocaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "bad locale code `{}` (2 to {MAX_LEN} of a-z, A-Z, 0-9 and -)",
            self.0
        )
    }
}

impl std::error::Error for ParseLocaleError {}

impl FromStr for Locale {
    type Err = ParseLocaleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Locale::parse_bytes(s.as_bytes()).ok_or_else(|| ParseLocaleError(s.to_owned()))
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Locale {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Locale {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_roundtrip() {
        for c in ["en", "pt-BR", "zh", "zh-Hant-TW", "ko"] {
            let l: Locale = c.parse().unwrap();
            assert_eq!(l.code(), c);
            assert_eq!(l.to_string(), c);
        }
        assert_eq!("pt-BR".parse::<Locale>().unwrap().language(), "pt");
        assert_eq!(Locale::EN.code(), "en");
        for bad in ["", "p", "1a", "pt_BR", "pt BR", "abcdefghijklmnop"] {
            assert!(bad.parse::<Locale>().is_err(), "{bad}");
        }
    }
}
