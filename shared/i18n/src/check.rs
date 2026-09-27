//! Text checks used by `KeyTable::from_source`, `Bundle::from_text` and the
//! `validate` tool. Every problem is collected; nothing stops at the first.

use crate::keys::{KeyInfo, KeyTable};
use crate::message::RawMessage;
use crate::source::{parse_file, valid_key};
use crate::{Bundle, Locale, Message, MessageError, PluralCategory, required_plurals};
use base::Fnv64;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextProblemKind {
    BadFile(String),
    BadKey,
    DuplicateKey,
    /// Present in `en`, absent in this locale.
    MissingKey,
    /// Present in this locale, absent in `en` (warning).
    UnknownKey,
    /// Made for an older English text (`review: true`, warning).
    NeedsReview,
    Parse(MessageError),
    PlaceholderMismatch {
        expected: Vec<String>,
        found: Vec<String>,
    },
    MissingPluralCategory {
        arg: String,
        category: PluralCategory,
    },
    TooLong {
        max: u32,
        got: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextProblem {
    pub locale: Locale,
    /// File name, empty when the problem is not tied to one file.
    pub file: String,
    pub key: Option<String>,
    pub kind: TextProblemKind,
}

impl TextProblem {
    pub fn severity(&self) -> Severity {
        match self.kind {
            TextProblemKind::UnknownKey | TextProblemKind::NeedsReview => Severity::Warning,
            _ => Severity::Error,
        }
    }
}

impl fmt::Display for TextProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity() {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{sev}: [{}]", self.locale)?;
        if !self.file.is_empty() {
            write!(f, " {}", self.file)?;
        }
        if let Some(k) = &self.key {
            write!(f, " `{k}`")?;
        }
        f.write_str(": ")?;
        match &self.kind {
            TextProblemKind::BadFile(e) => write!(f, "cannot read file: {e}"),
            TextProblemKind::BadKey => f.write_str("key must be `domain.id.field` in [a-z0-9_]"),
            TextProblemKind::DuplicateKey => f.write_str("key defined more than once"),
            TextProblemKind::MissingKey => f.write_str("missing translation"),
            TextProblemKind::UnknownKey => f.write_str("key does not exist in `en`"),
            TextProblemKind::NeedsReview => f.write_str("English text changed: review this translation"),
            TextProblemKind::Parse(e) => write!(f, "{e}"),
            TextProblemKind::PlaceholderMismatch { expected, found } => {
                write!(f, "placeholders {found:?} differ from `en` {expected:?}")
            }
            TextProblemKind::MissingPluralCategory { arg, category } => {
                write!(f, "plural `{arg}` needs case `{category}`")
            }
            TextProblemKind::TooLong { max, got } => {
                write!(f, "{got} characters, limit is {max}")
            }
        }
    }
}

/// Checks one translation against the source text: same placeholder names.
pub fn check_placeholders(source: &str, translation: &str) -> Result<(), TextProblemKind> {
    let a = RawMessage::parse(source).map_err(TextProblemKind::Parse)?;
    let b = RawMessage::parse(translation).map_err(TextProblemKind::Parse)?;
    compare_names(a.names(), b.names())
}

fn compare_names(expected: Vec<String>, found: Vec<String>) -> Result<(), TextProblemKind> {
    let mut e = expected.clone();
    let mut f = found.clone();
    e.sort();
    f.sort();
    if e == f {
        Ok(())
    } else {
        Err(TextProblemKind::PlaceholderMismatch { expected, found })
    }
}

/// All problems of one locale's files against the key table.
pub fn check_locale(keys: &KeyTable, locale: Locale, files: &[(&str, &str)]) -> Vec<TextProblem> {
    build_bundle(keys, locale, files).1
}

struct Collector {
    locale: Locale,
    problems: Vec<TextProblem>,
}

impl Collector {
    fn push(&mut self, file: &str, key: Option<&str>, kind: TextProblemKind) {
        self.problems.push(TextProblem {
            locale: self.locale,
            file: file.to_owned(),
            key: key.map(str::to_owned),
            kind,
        });
    }
}

pub(crate) fn build_source(files: &[(&str, &str)]) -> (KeyTable, Vec<TextProblem>) {
    let mut c = Collector {
        locale: Locale::SOURCE,
        problems: Vec::new(),
    };
    let mut entries: BTreeMap<String, (String, KeyInfo)> = BTreeMap::new();
    for (file, text) in files {
        let parsed = match parse_file(text) {
            Ok(p) => p,
            Err(e) => {
                c.push(file, None, TextProblemKind::BadFile(e));
                continue;
            }
        };
        for e in parsed {
            if !valid_key(&e.key) {
                c.push(file, Some(&e.key), TextProblemKind::BadKey);
                continue;
            }
            if entries.contains_key(&e.key) {
                c.push(file, Some(&e.key), TextProblemKind::DuplicateKey);
                continue;
            }
            let params = match RawMessage::parse(&e.text) {
                Ok(m) => m.names(),
                Err(err) => {
                    c.push(file, Some(&e.key), TextProblemKind::Parse(err));
                    Vec::new()
                }
            };
            if params.len() > usize::from(u8::MAX) {
                c.push(
                    file,
                    Some(&e.key),
                    TextProblemKind::Parse(MessageError::TooManyPlaceholders),
                );
            }
            let info = KeyInfo {
                params,
                secret: e.secret,
                max_chars: e.max_chars,
            };
            entries.insert(e.key, (e.text, info));
        }
    }
    let mut hash = Fnv64::new();
    let mut keys = Vec::with_capacity(entries.len());
    let mut info = Vec::with_capacity(entries.len());
    for (key, (text, i)) in entries {
        hash.write_str(&key);
        hash.write_str(&text);
        hash.write(&[u8::from(i.secret)]);
        keys.push(key);
        info.push(i);
    }
    let table = KeyTable {
        keys,
        info,
        hash: hash.finish(),
    };
    (table, c.problems)
}

pub(crate) fn build_bundle(
    keys: &KeyTable,
    locale: Locale,
    files: &[(&str, &str)],
) -> (Bundle, Vec<TextProblem>) {
    let mut c = Collector {
        locale,
        problems: Vec::new(),
    };
    let mut messages: Vec<Option<Message>> = vec![None; keys.len()];
    let mut seen = vec![false; keys.len()];
    for (file, text) in files {
        let parsed = match parse_file(text) {
            Ok(p) => p,
            Err(e) => {
                c.push(file, None, TextProblemKind::BadFile(e));
                continue;
            }
        };
        for e in parsed {
            let key = Some(e.key.as_str());
            let Some(id) = keys.id(&e.key) else {
                c.push(file, key, TextProblemKind::UnknownKey);
                continue;
            };
            let (Some(slot), Some(seen), Some(info)) = (
                messages.get_mut(id.index()),
                seen.get_mut(id.index()),
                keys.info.get(id.index()),
            ) else {
                continue;
            };
            if std::mem::replace(seen, true) {
                c.push(file, key, TextProblemKind::DuplicateKey);
                continue;
            }
            if e.review && locale != Locale::SOURCE {
                c.push(file, key, TextProblemKind::NeedsReview);
            }
            let raw = match RawMessage::parse(&e.text) {
                Ok(m) => m,
                Err(err) => {
                    c.push(file, key, TextProblemKind::Parse(err));
                    continue;
                }
            };
            if let Some(max) = info.max_chars {
                let got = raw.visible_chars();
                if got > max as usize {
                    c.push(file, key, TextProblemKind::TooLong { max, got });
                }
            }
            if let Err(kind) = compare_names(info.params.clone(), raw.names()) {
                c.push(file, key, kind);
                continue;
            }
            let mut plurals_ok = true;
            for (arg, cats) in raw.plurals() {
                for &category in required_plurals(locale) {
                    if !cats.contains(&category) {
                        plurals_ok = false;
                        c.push(
                            file,
                            key,
                            TextProblemKind::MissingPluralCategory {
                                arg: arg.clone(),
                                category,
                            },
                        );
                    }
                }
            }
            match raw.resolve(&info.params) {
                Ok(m) if plurals_ok => *slot = Some(m),
                Ok(_) => {}
                Err(err) => c.push(file, key, TextProblemKind::Parse(err)),
            }
        }
    }
    for (i, was_seen) in seen.iter().enumerate() {
        if !was_seen {
            let key = keys.keys.get(i).map(String::as_str);
            c.push("", key, TextProblemKind::MissingKey);
        }
    }
    let bundle = Bundle {
        locale,
        content_hash: keys.hash,
        messages,
        secret: keys.info.iter().map(|i| i.secret).collect(),
    };
    (bundle, c.problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: &str = r#"{
        "loot.coins": (text: "{count, plural, one {# coin} other {# coins}}"),
        "system.raised": (text: "{name} rose to {value}.", max_chars: 30),
    }"#;

    fn keys() -> KeyTable {
        KeyTable::from_source(&[("items.ron", EN)]).unwrap()
    }

    fn kinds(locale: Locale, text: &str) -> Vec<TextProblemKind> {
        check_locale(&keys(), locale, &[("items.ron", text)])
            .into_iter()
            .map(|p| p.kind)
            .collect()
    }

    #[test]
    fn source_is_clean() {
        assert!(kinds(Locale::EN, EN).is_empty());
    }

    #[test]
    fn russian_needs_few_and_many() {
        let k = kinds(
            Locale::from_static("ru"),
            r#"{
                "loot.coins": "{count, plural, one {# монета} other {# монеты}}",
                "system.raised": "{name}: {value}.",
            }"#,
        );
        assert_eq!(
            k,
            [
                TextProblemKind::MissingPluralCategory {
                    arg: "count".into(),
                    category: PluralCategory::Few
                },
                TextProblemKind::MissingPluralCategory {
                    arg: "count".into(),
                    category: PluralCategory::Many
                },
            ]
        );
    }

    #[test]
    fn missing_unknown_and_placeholders() {
        let k = kinds(
            Locale::from_static("de"),
            r#"{
                "system.raised": "{name} stieg auf {wert}.",
                "system.extra": "x",
            }"#,
        );
        assert_eq!(k.len(), 3, "{k:?}");
        assert!(matches!(k[0], TextProblemKind::PlaceholderMismatch { .. }));
        assert_eq!(k[1], TextProblemKind::UnknownKey);
        assert_eq!(k[2], TextProblemKind::MissingKey);
    }

    #[test]
    fn too_long_and_duplicate() {
        let k = kinds(
            Locale::from_static("es"),
            r#"{
                "loot.coins": "{count, plural, one {# moneda} other {# monedas}}",
                "loot.coins": "{count, plural, one {# moneda} other {# monedas}}",
                "system.raised": "{name} subió hasta el valor extraordinario de {value}.",
            }"#,
        );
        assert_eq!(k[0], TextProblemKind::DuplicateKey);
        assert!(matches!(k[1], TextProblemKind::TooLong { max: 30, .. }));
    }

    #[test]
    fn source_problems() {
        let err = KeyTable::from_source(&[
            ("a.ron", r#"{ "Bad.Key": "x", "ok.key": "{oops" }"#),
            ("b.ron", r#"{ "ok.key": "again" }"#),
            ("c.ron", "not ron"),
        ])
        .unwrap_err();
        let crate::I18nError::Text(p) = err else { panic!() };
        let k: Vec<_> = p.into_iter().map(|p| p.kind).collect();
        assert_eq!(k[0], TextProblemKind::BadKey);
        assert!(matches!(k[1], TextProblemKind::Parse(_)));
        assert_eq!(k[2], TextProblemKind::DuplicateKey);
        assert!(matches!(k[3], TextProblemKind::BadFile(_)));
    }

    #[test]
    fn check_placeholders_api() {
        assert!(check_placeholders("{a} {b}", "{b} {a}").is_ok());
        assert!(check_placeholders("{a}", "{a} {b}").is_err());
    }
}
