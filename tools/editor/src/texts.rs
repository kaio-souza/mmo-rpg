//! Text files (`content/text/<locale>/<file>.ron`): the key x locale table
//! and writing one entry back, keeping key order, header and format.

use serde::de::{Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Full {
    pub text: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub max_chars: Option<u32>,
    #[serde(default)]
    pub secret: bool,
    /// Made for an older English text.
    #[serde(default)]
    pub review: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Entry {
    Plain(String),
    Full(Full),
}

impl Entry {
    /// The same translation, marked as made for an older English text.
    pub fn to_review(&self) -> Entry {
        match self {
            Entry::Plain(t) => Entry::Full(Full {
                text: t.clone(),
                review: true,
                ..Full::default()
            }),
            Entry::Full(f) => Entry::Full(Full {
                review: true,
                ..f.clone()
            }),
        }
    }

    pub fn review(&self) -> bool {
        matches!(self, Entry::Full(f) if f.review)
    }

    pub fn text(&self) -> &str {
        match self {
            Entry::Plain(t) => t,
            Entry::Full(f) => &f.text,
        }
    }
}

/// A text file in file order.
#[derive(Debug, Default)]
pub struct TextFile {
    pub header: String,
    pub entries: Vec<(String, Entry)>,
}

struct Ordered(Vec<(String, Entry)>);

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Ordered;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map from text key to text")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Ordered, M::Error> {
                let mut out = Vec::new();
                while let Some(kv) = m.next_entry::<String, Entry>()? {
                    out.push(kv);
                }
                Ok(Ordered(out))
            }
        }
        d.deserialize_map(V)
    }
}

impl TextFile {
    pub fn parse(text: &str) -> Result<TextFile, String> {
        let Ordered(entries) = ron::from_str(text).map_err(|e| e.to_string())?;
        Ok(TextFile {
            header: defs::editor::header(text),
            entries,
        })
    }

    /// Sets `key`; a new key goes to the end. `None` removes it (only used
    /// for an empty translation: the key stays in `en`).
    pub fn set(&mut self, key: &str, entry: Option<Entry>) {
        let pos = self.entries.iter().position(|(k, _)| k == key);
        match (pos, entry) {
            (Some(i), Some(e)) => {
                if let Some(slot) = self.entries.get_mut(i) {
                    slot.1 = e;
                }
            }
            (None, Some(e)) => self.entries.push((key.to_owned(), e)),
            (Some(i), None) => {
                self.entries.remove(i);
            }
            (None, None) => {}
        }
    }

    /// Same format as the hand-written files: one entry per line.
    pub fn render(&self) -> String {
        let q = |s: &str| ron::to_string(s).unwrap_or_else(|_| "\"\"".into());
        let mut out = self.header.clone();
        out.push_str("{\n");
        for (k, e) in &self.entries {
            let value = match e {
                Entry::Plain(t) => q(t),
                Entry::Full(f) => {
                    let mut v = format!("(text: {}", q(&f.text));
                    if let Some(n) = &f.note {
                        v.push_str(&format!(", note: {}", q(n)));
                    }
                    if let Some(m) = f.max_chars {
                        v.push_str(&format!(", max_chars: {m}"));
                    }
                    if f.secret {
                        v.push_str(", secret: true");
                    }
                    if f.review {
                        v.push_str(", review: true");
                    }
                    v.push(')');
                    v
                }
            };
            out.push_str(&format!("    {}: {value},\n", q(k)));
        }
        out.push_str("}\n");
        out
    }
}

/// One row of the translation table.
#[derive(Debug, Serialize)]
pub struct Row {
    pub key: String,
    /// File name under `text/<locale>/`.
    pub file: String,
    pub note: Option<String>,
    pub max_chars: Option<u32>,
    pub secret: bool,
    /// Locale code -> text; absent: missing.
    pub texts: BTreeMap<String, String>,
    /// Locales whose text was made for an older English text.
    pub review: Vec<String>,
}

/// Every key of `en` with its text in each locale.
pub fn table(files: &[(String, String)]) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    let parsed: Vec<(&str, &str, TextFile)> = files
        .iter()
        .filter_map(|(p, t)| {
            let rest = p.strip_prefix("text/")?;
            let (locale, file) = rest.split_once('/')?;
            Some((locale, file, TextFile::parse(t).ok()?))
        })
        .collect();
    for (locale, file, tf) in &parsed {
        if *locale != "en" {
            continue;
        }
        for (k, e) in &tf.entries {
            let (note, max_chars, secret) = match e {
                Entry::Plain(_) => (None, None, false),
                Entry::Full(f) => (f.note.clone(), f.max_chars, f.secret),
            };
            index.insert(k.clone(), rows.len());
            rows.push(Row {
                key: k.clone(),
                file: (*file).to_owned(),
                note,
                max_chars,
                secret,
                texts: BTreeMap::new(),
                review: Vec::new(),
            });
        }
    }
    for (locale, _, tf) in &parsed {
        for (k, e) in &tf.entries {
            if let Some(row) = index.get(k).and_then(|i| rows.get_mut(*i)) {
                row.texts.insert((*locale).to_owned(), e.text().to_owned());
                if e.review() {
                    row.review.push((*locale).to_owned());
                }
            }
        }
    }
    rows
}

/// File a new key goes to: the `en` file whose keys share its first part
/// (`item.*` -> `items.ron`), else `misc.ron`.
pub fn file_for(files: &[(String, String)], key: &str) -> String {
    let prefix = key.split('.').next().unwrap_or("");
    for row in table(files) {
        if row.key.split('.').next() == Some(prefix) {
            return row.file;
        }
    }
    "misc.ron".to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn render_keeps_order_header_and_format() {
        let src = "// Texts.\n{\n    \"a.b\": (text: \"Hi \\\"x\\\"\", note: \"n\", max_chars: 10),\n    \"a.c\": \"Plain\",\n}\n";
        let mut f = TextFile::parse(src).unwrap();
        assert_eq!(f.render(), src);
        f.set("a.d", Some(Entry::Plain("New".into())));
        f.set("a.c", None);
        let out = f.render();
        assert!(out.ends_with("    \"a.d\": \"New\",\n}\n"), "{out}");
        assert!(!out.contains("a.c"));
    }

    #[test]
    fn every_real_text_file_renders_back_the_same_entries() {
        let store = crate::store::Store::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content"),
        );
        for (p, t) in store
            .files()
            .unwrap()
            .iter()
            .filter(|(p, _)| p.starts_with("text/") && p.matches('/').count() == 2)
        {
            let f = TextFile::parse(t).unwrap_or_else(|e| panic!("{p}: {e}"));
            let again = TextFile::parse(&f.render()).unwrap();
            assert_eq!(again.entries, f.entries, "{p}");
        }
    }
}
