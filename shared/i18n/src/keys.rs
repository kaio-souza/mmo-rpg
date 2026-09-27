use crate::check;
use crate::{I18nError, Severity};
use base::TextId;

/// Text key <-> `TextId`, built from the source (`en`) files.
///
/// `TextId` is the position of the key in the sorted key list, so the same
/// content always gives the same IDs. Also stores, per key, the placeholder
/// names in order of first appearance in `en` (argument positions of
/// `TextRef`), the `secret` flag and `max_chars`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyTable {
    pub(crate) keys: Vec<String>,
    pub(crate) info: Vec<KeyInfo>,
    pub(crate) hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyInfo {
    pub params: Vec<String>,
    pub secret: bool,
    pub max_chars: Option<u32>,
}

impl KeyTable {
    /// `files`: `(file name, file text)` of the `en` folder.
    pub fn from_source(files: &[(&str, &str)]) -> Result<KeyTable, I18nError> {
        let (table, problems) = check::build_source(files);
        let errors: Vec<_> = problems
            .into_iter()
            .filter(|p| p.severity() == Severity::Error)
            .collect();
        if errors.is_empty() {
            Ok(table)
        } else {
            Err(I18nError::Text(errors))
        }
    }

    pub fn id(&self, key: &str) -> Option<TextId> {
        let i = self.keys.binary_search_by(|k| k.as_str().cmp(key)).ok()?;
        u32::try_from(i).ok().map(TextId::new)
    }

    pub fn key(&self, id: TextId) -> Option<&str> {
        self.keys.get(id.index()).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Placeholder names; position `i` is argument `i` of a `TextRef`.
    pub fn params(&self, id: TextId) -> &[String] {
        self.info.get(id.index()).map_or(&[], |i| i.params.as_slice())
    }

    pub fn is_secret(&self, id: TextId) -> bool {
        self.info.get(id.index()).is_some_and(|i| i.secret)
    }

    /// Hash of every key and its `en` text: identifies a text pack version.
    pub fn content_hash(&self) -> u64 {
        self.hash
    }

    pub fn iter(&self) -> impl Iterator<Item = (TextId, &str)> {
        self.keys
            .iter()
            .enumerate()
            .map(|(i, k)| (TextId::new(i as u32), k.as_str()))
    }
}
