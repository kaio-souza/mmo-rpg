//! Content text on the client: the text pack of the chosen locale
//! for the running content hash, from the disk cache or asked to the server
//! once, then decoded with `i18n`.
//!
//! No text on screen until fonts arrive (phase 5): dialogue goes to the log.

use base::{Locale, TextRef};
use client_state::ReceivedTextPack;
use i18n::Bundle;
use std::path::PathBuf;
use tracing::{info, warn};

pub struct Texts {
    locale: Locale,
    /// The pack in use and the content hash (`Welcome`) it belongs to. The
    /// bundle's own hash covers only the text keys, so the content hash is
    /// what names the pack on the wire and in the cache.
    bundle: Option<(u64, Bundle)>,
    /// Content hash of the pack already asked for, so it is asked once.
    requested: Option<u64>,
    /// `<cache>/<game>/text`; `None` keeps packs only in memory.
    cache_dir: Option<PathBuf>,
}

pub enum Need {
    /// Ask the server with `RequestTextPack`.
    Request(Locale),
    Nothing,
}

impl Texts {
    pub fn new(locale: Locale) -> Texts {
        Texts::with_cache(locale, default_cache_dir())
    }

    fn with_cache(locale: Locale, cache_dir: Option<PathBuf>) -> Texts {
        Texts {
            locale,
            bundle: None,
            requested: None,
            cache_dir,
        }
    }

    fn cache_file(&self, content_hash: u64) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        Some(dir.join(format!("{content_hash:016x}-{}.pack", self.locale)))
    }

    /// Called every frame with the content hash from `Welcome` (0 before it):
    /// loads the cached pack, or says to ask for it once.
    pub fn need(&mut self, content_hash: u64) -> Need {
        if content_hash == 0 || self.bundle.as_ref().is_some_and(|(h, _)| *h == content_hash) {
            return Need::Nothing;
        }
        if self.requested == Some(content_hash) {
            return Need::Nothing;
        }
        if let Some(bytes) = self.cache_file(content_hash).and_then(|p| std::fs::read(p).ok())
            && let Ok(bundle) = Bundle::from_pack(&bytes)
            && bundle.locale() == self.locale
        {
            info!(locale = %self.locale, "text pack from the cache");
            self.bundle = Some((content_hash, bundle));
            return Need::Nothing;
        }
        self.requested = Some(content_hash);
        Need::Request(self.locale)
    }

    /// Before `Welcome` the content hash is unknown: the selection and
    /// creation screens ask for the pack once to name the look options.
    pub fn need_before_game(&mut self) -> Need {
        if self.bundle.is_some() || self.requested.is_some() {
            return Need::Nothing;
        }
        self.requested = Some(0);
        Need::Request(self.locale)
    }

    /// A pack from the server: kept and cached if it matches.
    pub fn received(&mut self, pack: ReceivedTextPack, content_hash: u64) {
        if pack.locale != self.locale || pack.content_hash != content_hash {
            return;
        }
        let bundle = match Bundle::from_pack(&pack.data) {
            // A language the server lacks comes in English.
            Ok(b) if b.locale() == self.locale || b.locale() == Locale::SOURCE => b,
            Ok(_) => return warn!("text pack for another locale"),
            Err(e) => return warn!(error = %e, "bad text pack"),
        };
        if let Some(path) = self.cache_file(content_hash) {
            let saved = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, &pack.data));
            if let Err(e) = saved {
                warn!(error = %e, path = %path.display(), "cannot cache the text pack");
            }
        }
        info!(locale = %self.locale, "text pack received");
        self.bundle = Some((content_hash, bundle));
    }

    /// Translated text, or the numeric id until the pack arrives.
    pub fn show(&self, r: &TextRef) -> String {
        match &self.bundle {
            Some((_, b)) => i18n::format(b, r),
            None => format!("#{}", r.id.get()),
        }
    }
}

/// `<cache>/<game>/text`, holding `<hash>-<locale>.pack` files:
/// `$XDG_CACHE_HOME` or `~/.cache` on Linux, `%LOCALAPPDATA%` on Windows.
fn default_cache_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    }?;
    Some(base.join(client_ui::GAME_ID).join("text"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base::TextRef;
    use i18n::KeyTable;

    fn pack(locale: Locale) -> (Vec<u8>, TextRef) {
        let src = [("a.ron", r#"{ "npc.guide.name": "Guide" }"#)];
        let keys = KeyTable::from_source(&src).unwrap();
        let id = keys.id("npc.guide.name").unwrap();
        let files = [(
            "a.ron",
            if locale == Locale::EN {
                src[0].1
            } else {
                r#"{ "npc.guide.name": "Guia" }"#
            },
        )];
        let bundle = Bundle::from_text(locale, &keys, &files).unwrap();
        (bundle.to_pack(false).unwrap(), TextRef::new(id))
    }

    // The content hash of `Welcome` covers every content file, not only the
    // text keys, so it never equals the bundle's own hash.
    #[test]
    fn pack_is_kept_under_the_content_hash_and_read_back_from_the_cache() {
        let dir = std::env::temp_dir().join(format!("client-text-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let content_hash = 0x1234;
        let (data, name) = pack(Locale::from_static("pt-BR"));

        let mut t = Texts::with_cache(Locale::from_static("pt-BR"), Some(dir.clone()));
        assert!(matches!(t.need(content_hash), Need::Request(l) if l == Locale::from_static("pt-BR")));
        assert!(matches!(t.need(content_hash), Need::Nothing), "asked once");
        assert_eq!(t.show(&name), format!("#{}", name.id.get()));
        t.received(
            ReceivedTextPack {
                locale: Locale::from_static("pt-BR"),
                content_hash,
                data,
            },
            content_hash,
        );
        assert_eq!(t.show(&name), "Guia");

        let mut again = Texts::with_cache(Locale::from_static("pt-BR"), Some(dir.clone()));
        assert!(
            matches!(again.need(content_hash), Need::Nothing),
            "from the cache"
        );
        assert_eq!(again.show(&name), "Guia");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_of_another_locale_is_ignored_but_english_is_the_fallback() {
        let (data, name) = pack(Locale::from_static("de"));
        let mut t = Texts::with_cache(Locale::from_static("pt-BR"), None);
        let got = |data| ReceivedTextPack {
            locale: Locale::from_static("pt-BR"),
            content_hash: 7,
            data,
        };
        t.received(got(data), 7);
        assert_eq!(t.show(&name), format!("#{}", name.id.get()));
        // A language the server lacks comes in English.
        let (en, _) = pack(Locale::EN);
        t.received(got(en), 7);
        assert_eq!(t.show(&name), "Guide");
    }
}
