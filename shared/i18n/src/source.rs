//! Text files: a RON map from key to entry.
//!
//! ```ron
//! {
//!     "item.iron_sword.name": (text: "Iron Sword", note: "Starter weapon", max_chars: 24),
//!     "loot.coins": "{count, plural, one {# coin} other {# coins}}",
//! }
//! ```
//!
//! `note`, `max_chars` and `secret` are read from `en` only. `review: true`
//! in a translation marks it as made for an older English text (set by
//! `tools/editor`; `validate` warns).

use serde::Deserialize;
use serde::de::{Deserializer, MapAccess, Visitor};
use std::fmt;

pub(crate) struct SourceEntry {
    pub key: String,
    pub text: String,
    pub max_chars: Option<u32>,
    pub secret: bool,
    /// Translation of an older English text.
    pub review: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Entry {
    Plain(String),
    Full(FullEntry),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FullEntry {
    text: String,
    #[serde(default)]
    #[allow(dead_code)] // only for translators
    note: Option<String>,
    #[serde(default)]
    max_chars: Option<u32>,
    #[serde(default)]
    secret: bool,
    #[serde(default)]
    review: bool,
}

/// Map entries in file order, keeping duplicates so they can be reported.
struct Entries(Vec<(String, Entry)>);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Entries;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map from text key to text")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Entries, M::Error> {
                let mut out = Vec::new();
                while let Some(kv) = m.next_entry::<String, Entry>()? {
                    out.push(kv);
                }
                Ok(Entries(out))
            }
        }
        d.deserialize_map(V)
    }
}

pub(crate) fn parse_file(text: &str) -> Result<Vec<SourceEntry>, String> {
    let Entries(entries) = ron::from_str(text).map_err(|e| e.to_string())?;
    Ok(entries
        .into_iter()
        .map(|(key, e)| match e {
            Entry::Plain(text) => SourceEntry {
                key,
                text,
                max_chars: None,
                secret: false,
                review: false,
            },
            Entry::Full(f) => SourceEntry {
                key,
                text: f.text,
                max_chars: f.max_chars,
                secret: f.secret,
                review: f.review,
            },
        })
        .collect())
}

/// `domain.id.field`: lowercase ASCII, digits and `_`, at least two segments.
pub(crate) fn valid_key(key: &str) -> bool {
    let mut segments = 0;
    for seg in key.split('.') {
        if seg.is_empty()
            || !seg
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return false;
        }
        segments += 1;
    }
    segments >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_entry_forms() {
        let e = parse_file(
            r#"{
                "a.b": (text: "Iron Sword", note: "n", max_chars: 24),
                "a.c": "plain",
                "a.d": (text: "hidden", secret: true),
                "a.b": "dup",
            }"#,
        )
        .unwrap();
        assert_eq!(e.len(), 4);
        assert_eq!(e[0].max_chars, Some(24));
        assert_eq!(e[1].text, "plain");
        assert!(e[2].secret);
        assert_eq!(e[3].key, "a.b");
    }

    #[test]
    fn rejects_unknown_field() {
        assert!(parse_file(r#"{ "a.b": (text: "x", colour: 1) }"#).is_err());
    }

    #[test]
    fn keys() {
        assert!(valid_key("item.iron_sword.name"));
        assert!(valid_key("loot.coins"));
        for bad in ["item", "Item.x", "a..b", "a.b.", "a.b-c", ""] {
            assert!(!valid_key(bad), "{bad}");
        }
    }
}
