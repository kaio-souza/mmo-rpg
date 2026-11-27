//! Content on the server: load and validate `content/`, compile scripts,
//! build what clients receive (text pack parts, `Definitions`), and watch
//! the folder for changes. Reloading never stops the server:
//! content with problems is logged and the running content stays.

use crate::translation;
use base::Locale;
use bytes::Bytes;
use i18n::KeyTable;
use sim::Content;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tracing::{info, warn};

/// How often `--watch-content` looks at the files.
const WATCH_EVERY: Duration = Duration::from_secs(2);

/// The running content of the process, handed from the gateway to the zones
/// on reload (one process today; separate processes load it themselves).
pub type ContentSlot = Arc<Mutex<Arc<Content>>>;

pub fn slot_get(slot: &ContentSlot) -> Arc<Content> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Everything the gateway sends about one content version, encoded once.
pub struct ContentFrames {
    pub hash: u64,
    pub keys: KeyTable,
    /// `TextPack` frames of each locale, in part order.
    pub text_packs: BTreeMap<Locale, Vec<Bytes>>,
    pub definitions: Bytes,
    /// `Appearances`, sent with `Session` (phase 5).
    pub appearances: Bytes,
    /// Creation lists: checks `CreateCharacter` and maps saved ids.
    pub appearance: defs::AppearanceDef,
    /// Full reason keys an admin may use (`moderation.reason.*`).
    pub reasons: Vec<String>,
    /// Public rankings (`/ranking`).
    pub rankings: Vec<defs::RankingDef>,
    /// Unique titles and classes (`title:<id>`, `class:<id>`) with their
    /// days before an heir may take them.
    pub unique: BTreeMap<String, u16>,
    /// Zones that are instances, with their rule and their exit's zone.
    pub instances: BTreeMap<base::ZoneId, (defs::InstanceRule, base::ZoneId)>,
    /// Dungeons (lockouts, clears, `/firsts`).
    pub dungeons: Vec<defs::DungeonDef>,
}

/// `allow_missing_text` (dev only): a key missing in a locale shows the
/// English text and is logged, so content made in the editor can be played
/// before it is translated.
pub fn load(dir: &Path, allow_missing_text: bool) -> Result<Content, String> {
    let mut catalog = defs::load(dir).map_err(|e| format!("content: {e}"))?;
    if allow_missing_text {
        let filled = catalog.fill_missing_text();
        if !filled.is_empty() {
            let keys: std::collections::BTreeSet<&str> = filled.iter().map(|(_, k)| k.as_str()).collect();
            warn!(keys = keys.len(), first = ?keys.iter().take(5).collect::<Vec<_>>(), "missing translations shown in English (--allow-missing-text)");
        }
    }
    let catalog = catalog.validate().map_err(|problems| {
        let lines: Vec<String> = problems.iter().map(ToString::to_string).collect();
        format!("content has problems:\n{}", lines.join("\n"))
    })?;
    Ok(Content::new(catalog))
}

pub fn frames(content: &Content) -> Result<ContentFrames, String> {
    frames_with_part(content, protocol::TEXT_PACK_PART)
}

/// `part_size` is a parameter only so tests can force many parts.
pub fn frames_with_part(content: &Content, part_size: usize) -> Result<ContentFrames, String> {
    let catalog = &content.catalog;
    let mut text_packs = BTreeMap::new();
    for locale in catalog.locales().iter().map(|d| d.locale) {
        let Some(bundle) = catalog.bundle(locale) else {
            continue;
        };
        // Without secret texts.
        let data = bundle
            .to_pack(false)
            .map_err(|e| format!("text pack {locale}: {e}"))?;
        let mut parts = Vec::new();
        for msg in protocol::text_pack_parts(locale, catalog.content_hash(), &data, part_size) {
            let mut frame = Vec::new();
            protocol::encode(&msg, &mut frame).map_err(|e| format!("text pack {locale}: {e}"))?;
            parts.push(Bytes::from(frame));
        }
        text_packs.insert(locale, parts);
    }
    Ok(ContentFrames {
        hash: catalog.content_hash(),
        keys: catalog.keys().clone(),
        text_packs,
        definitions: translation::definitions(catalog).map_err(|e| format!("definitions: {e}"))?,
        appearances: translation::appearances(catalog).map_err(|e| format!("appearances: {e}"))?,
        appearance: catalog.appearance().clone(),
        reasons: catalog.moderation_reasons().to_vec(),
        rankings: catalog.rankings().to_vec(),
        unique: crate::gateway::unique_keys(catalog),
        instances: crate::gateway::instance_rules(catalog),
        dungeons: catalog.dungeons().to_vec(),
    })
}

/// Path, size and modification time of every file: a change in any of them
/// means "load again".
fn fingerprint(dir: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64, Option<SystemTime>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            match e.metadata() {
                Ok(m) if m.is_dir() => walk(&path, out),
                Ok(m) => out.push((path, m.len(), m.modified().ok())),
                Err(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

/// A new content version that passed every check.
pub struct Reloaded {
    pub content: Arc<Content>,
    pub frames: ContentFrames,
}

/// Looks at `dir` every 2 s; on a change, loads it in the blocking pool and
/// sends what passed. Problems go to the log and the running content stays.
pub fn watch(dir: PathBuf, allow_missing_text: bool, tx: mpsc::UnboundedSender<Reloaded>) {
    tokio::spawn(async move {
        let d = dir.clone();
        let mut last = tokio::task::spawn_blocking(move || fingerprint(&d))
            .await
            .unwrap_or_default();
        let mut tick = tokio::time::interval(WATCH_EVERY);
        info!(dir = %dir.display(), "watching content for changes");
        loop {
            tick.tick().await;
            let d = dir.clone();
            let Ok(now) = tokio::task::spawn_blocking(move || fingerprint(&d)).await else {
                continue;
            };
            if now == last {
                continue;
            }
            last = now;
            let d = dir.clone();
            let loaded = tokio::task::spawn_blocking(move || {
                let content = load(&d, allow_missing_text)?;
                let frames = frames(&content)?;
                Ok::<_, String>(Reloaded {
                    content: Arc::new(content),
                    frames,
                })
            })
            .await;
            match loaded {
                Ok(Ok(r)) => {
                    info!(hash = %format!("{:016x}", r.frames.hash), "content changed: reloading");
                    if tx.send(r).is_err() {
                        return;
                    }
                }
                Ok(Err(e)) => warn!("content changed but was not loaded; the running content stays.\n{e}"),
                Err(e) => warn!(error = %e, "content load task failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_content() -> Content {
        // `true`: a text still to be translated shows in English, as the
        // dev server does (`--allow-missing-text`).
        load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content"),
            true,
        )
        .unwrap()
    }

    #[test]
    fn packs_split_into_parts_that_join() {
        let content = repo_content();
        let whole = frames(&content).unwrap();
        assert_eq!(whole.text_packs.len(), content.catalog.locales().len());
        let small = frames_with_part(&content, 1000).unwrap();
        let parts = &small.text_packs[&Locale::from_static("pt-BR")];
        assert!(parts.len() > 1);
        let mut joined = Vec::new();
        for (i, frame) in parts.iter().enumerate() {
            let (msg, _): (protocol::ServerMsg, _) =
                protocol::decode_frame(frame, protocol::MAX_SERVER_FRAME)
                    .unwrap()
                    .unwrap();
            let protocol::ServerMsg::TextPack {
                part, parts: n, data, ..
            } = msg
            else {
                panic!("not a text pack")
            };
            assert_eq!((usize::from(part), usize::from(n)), (i, parts.len()));
            joined.extend(data);
        }
        let bundle = i18n::Bundle::from_pack(&joined).unwrap();
        assert_eq!(bundle.locale(), Locale::from_static("pt-BR"));
    }
}
