use crate::check::{self, Severity};
use crate::{I18nError, KeyTable, Locale, Message};
use base::TextId;

/// Pack format version; bump when `Message` or `Pack` changes shape.
const PACK_FORMAT: u16 = 1;
/// Largest decompressed pack accepted (packs come from the network).
const MAX_PACK_BYTES: usize = 32 * 1024 * 1024;

/// Parsed messages of one locale, indexed by `TextId` (dense vector).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub(crate) locale: Locale,
    pub(crate) content_hash: u64,
    pub(crate) messages: Vec<Option<Message>>,
    /// Secret flag per key; empty for bundles read from a pack.
    pub(crate) secret: Vec<bool>,
}

#[derive(serde::Serialize)]
struct PackOut<'a> {
    format: u16,
    locale: Locale,
    content_hash: u64,
    messages: Vec<Option<&'a Message>>,
}

#[derive(serde::Deserialize)]
struct PackIn {
    format: u16,
    locale: Locale,
    content_hash: u64,
    messages: Vec<Option<Message>>,
}

impl Bundle {
    /// Builds from text already in memory (`(file name, file text)`).
    /// Fails on any error; warnings are ignored.
    pub fn from_text(locale: Locale, keys: &KeyTable, files: &[(&str, &str)]) -> Result<Bundle, I18nError> {
        let (bundle, problems) = check::build_bundle(keys, locale, files);
        let errors: Vec<_> = problems
            .into_iter()
            .filter(|p| p.severity() == Severity::Error)
            .collect();
        if errors.is_empty() {
            Ok(bundle)
        } else {
            Err(I18nError::Text(errors))
        }
    }

    /// Reads a pack made by `to_pack` (untrusted input: never panics).
    pub fn from_pack(pack: &[u8]) -> Result<Bundle, I18nError> {
        let size = pack
            .get(..4)
            .and_then(|b| <[u8; 4]>::try_from(b).ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| I18nError::Pack("truncated".into()))?;
        if size as usize > MAX_PACK_BYTES {
            return Err(I18nError::Pack(format!("too large: {size} bytes")));
        }
        let raw = lz4_flex::decompress_size_prepended(pack).map_err(|e| I18nError::Pack(e.to_string()))?;
        let p: PackIn = postcard::from_bytes(&raw).map_err(|e| I18nError::Pack(e.to_string()))?;
        if p.format != PACK_FORMAT {
            return Err(I18nError::Pack(format!("unsupported format {}", p.format)));
        }
        Ok(Bundle {
            locale: p.locale,
            content_hash: p.content_hash,
            messages: p.messages,
            secret: Vec::new(),
        })
    }

    /// Serialized and compressed pack. Secret texts are left out unless
    /// `include_secret` (the server keeps those).
    pub fn to_pack(&self, include_secret: bool) -> Result<Vec<u8>, I18nError> {
        let messages = self
            .messages
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let secret = self.secret.get(i).copied().unwrap_or(false);
                if secret && !include_secret {
                    None
                } else {
                    m.as_ref()
                }
            })
            .collect();
        let out = PackOut {
            format: PACK_FORMAT,
            locale: self.locale,
            content_hash: self.content_hash,
            messages,
        };
        let raw = postcard::to_stdvec(&out).map_err(|e| I18nError::Pack(e.to_string()))?;
        Ok(lz4_flex::compress_prepend_size(&raw))
    }

    pub fn get(&self, id: TextId) -> Option<&Message> {
        self.messages.get(id.index())?.as_ref()
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn content_hash(&self) -> u64 {
        self.content_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_roundtrip_drops_secrets() {
        let keys = KeyTable::from_source(&[(
            "a.ron",
            r#"{ "quest.a.title": "Open", "quest.b.title": (text: "Hidden", secret: true) }"#,
        )])
        .unwrap();
        let b = Bundle::from_text(
            Locale::EN,
            &keys,
            &[(
                "a.ron",
                r#"{ "quest.a.title": "Open", "quest.b.title": "Hidden" }"#,
            )],
        )
        .unwrap();
        let open = keys.id("quest.a.title").unwrap();
        let secret = keys.id("quest.b.title").unwrap();
        assert!(keys.is_secret(secret));

        let client = Bundle::from_pack(&b.to_pack(false).unwrap()).unwrap();
        assert!(client.get(open).is_some());
        assert!(client.get(secret).is_none());
        assert_eq!(client.content_hash(), keys.content_hash());

        let full = Bundle::from_pack(&b.to_pack(true).unwrap()).unwrap();
        assert!(full.get(secret).is_some());
    }

    #[test]
    fn bad_packs_are_errors() {
        for bad in [
            &[][..],
            &[1, 2],
            &[0xff, 0xff, 0xff, 0xff, 0],
            &[3, 0, 0, 0, 9, 9, 9],
        ] {
            assert!(Bundle::from_pack(bad).is_err());
        }
    }
}
