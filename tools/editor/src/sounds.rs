//! The Sounds screen: `assets/sounds/sounds.ron` (which sound
//! plays for each event) and the `.ogg` files under `assets/sounds/`.
//!
//! The editor never encodes audio: files arrive already in OGG Vorbis,
//! made by a content tool outside the build. Saving goes through
//! `defs::SoundConfig::check`, like `validate`.

use defs::{SOUND_LICENSES, SoundConfig, SoundEvent};
use serde_json::{Value, json};
use std::path::Path;

/// Largest sound file the page may send (the router's body limit).
pub const MAX_OGG_BYTES: usize = crate::art::MAX_PNG_BYTES;

fn config_path(assets: &Path) -> std::path::PathBuf {
    assets.join("sounds").join("sounds.ron")
}

/// Technique ids and zone names of the content, from its file paths
/// (`techniques/<id>.ron`, `zones/<name>/zone.ron`), for.
pub fn content_ids(files: &[(String, String)]) -> (Vec<String>, Vec<String>) {
    let mut techniques = Vec::new();
    let mut zones = Vec::new();
    for (path, _) in files {
        let parts: Vec<&str> = path.split('/').collect();
        match parts.as_slice() {
            ["techniques", f] => techniques.extend(f.strip_suffix(".ron").map(str::to_owned)),
            ["zones", z, "zone.ron"] => zones.push((*z).to_owned()),
            _ => {}
        }
    }
    techniques.sort();
    zones.sort();
    (techniques, zones)
}

/// Everything the screen shows: the bindings, the events the client
/// fires, the techniques and zones of the content, the files on disk and
/// the licenses allowed.
pub fn overview(assets: &Path, content: &[(String, String)]) -> Result<Value, String> {
    let (config, files) = defs::read_sounds(assets)?;
    let (techniques, zones) = content_ids(content);
    let mut warnings = config.missing_files(&files);
    warnings.extend(config.check_refs(
        techniques.iter().map(String::as_str),
        zones.iter().map(String::as_str),
    ));
    let events: Vec<Value> = SoundEvent::ALL
        .iter()
        .map(|e| json!({ "id": e.id(), "interface": e.interface() }))
        .collect();
    let folders: std::collections::BTreeSet<String> = files
        .iter()
        .filter_map(|f| f.strip_prefix("sounds/"))
        .filter_map(|f| f.split('/').next().filter(|_| f.contains('/')))
        .map(str::to_owned)
        .collect();
    Ok(json!({
        "config": config,
        "events": events,
        "files": files,
        "folders": folders,
        "licenses": SOUND_LICENSES,
        "warnings": warnings,
        "max_bytes": MAX_OGG_BYTES,
        "techniques": techniques,
        "zones": zones,
        "default_fade": defs::DEFAULT_MUSIC_FADE,
        "screens": defs::MUSIC_SCREENS,
    }))
}

/// Writes the whole `sounds.ron` in the fixed layout; refuses on any error.
pub fn save(assets: &Path, config: &SoundConfig) -> Result<(), String> {
    let errs = config.check();
    if !errs.is_empty() {
        return Err(errs.join(" · "));
    }
    let path = config_path(assets);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    crate::looks::write(&path, &config.to_ron())
}

/// True when `bytes` look like OGG Vorbis: an Ogg page whose first packet
/// is the Vorbis identification header.
fn is_ogg_vorbis(bytes: &[u8]) -> bool {
    let head = bytes.get(..64).unwrap_or(bytes);
    bytes.starts_with(b"OggS") && head.windows(7).any(|w| w == b"\x01vorbis")
}

/// Writes one uploaded `.ogg` as `assets/sounds/<folder>/<name>.ogg` (the
/// folder `music` writes `assets/music/<name>.ogg`) and returns that path
/// under `assets/`. Refuses what is not OGG Vorbis, a
/// file over [`MAX_OGG_BYTES`], and a name already there unless `replace`.
pub fn upload(
    assets: &Path,
    folder: &str,
    name: &str,
    bytes: &[u8],
    replace: bool,
) -> Result<String, String> {
    let rel = if folder == "music" {
        format!("music/{name}.ogg")
    } else {
        format!("sounds/{folder}/{name}.ogg")
    };
    if !defs::valid_sound_file(&rel) {
        return Err("folder and name must be [a-z0-9_]".into());
    }
    if bytes.len() > MAX_OGG_BYTES {
        return Err(format!("the file is over {} MB", MAX_OGG_BYTES / (1024 * 1024)));
    }
    if !is_ogg_vorbis(bytes) {
        return Err(
            "not an OGG Vorbis file: convert it first (art-src/generate_sound.py shows the ffmpeg command)"
                .into(),
        );
    }
    let path = assets.join(&rel);
    if path.exists() && !replace {
        return Err(format!("{rel} already exists"));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("ogg.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// The bytes of a sound file for the page to play; `None` for a path that
/// is not a sound path (nothing else of the disk is served).
pub fn read(assets: &Path, rel: &str) -> Option<Vec<u8>> {
    defs::valid_sound_file(rel).then(|| std::fs::read(assets.join(rel)).ok())?
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "editor-sounds-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn real() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/assets")
    }

    #[test]
    fn the_overview_lists_every_event_and_the_shipped_files() {
        let content = vec![
            ("techniques/cleave.ron".to_owned(), String::new()),
            ("zones/aurelia/zone.ron".to_owned(), String::new()),
            ("zones/aurelia/npcs.ron".to_owned(), String::new()),
        ];
        assert_eq!(
            content_ids(&content),
            (vec!["cleave".to_owned()], vec!["aurelia".to_owned()])
        );
        let v = overview(&real(), &content).unwrap();
        assert_eq!(v["events"].as_array().unwrap().len(), SoundEvent::ALL.len());
        assert!(
            v["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f == "sounds/ui/click_1.ogg")
        );
        assert!(v["folders"].as_array().unwrap().iter().any(|f| f == "ui"));
        // The shipped zone music names zones this sample does not have.
        assert!(
            v["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .all(|w| w.as_str().unwrap().contains("zone `"))
        );
    }

    #[test]
    fn uploads_take_only_ogg_vorbis_with_good_names_and_never_overwrite_by_accident() {
        let dir = tmp();
        let ogg = std::fs::read(real().join("sounds/ui/click_1.ogg")).unwrap();
        assert_eq!(
            upload(&dir, "ui", "beep", &ogg, false).unwrap(),
            "sounds/ui/beep.ogg"
        );
        assert!(
            upload(&dir, "ui", "beep", &ogg, false)
                .unwrap_err()
                .contains("exists")
        );
        assert!(upload(&dir, "ui", "beep", &ogg, true).is_ok());
        assert_eq!(
            upload(&dir, "music", "town", &ogg, false).unwrap(),
            "music/town.ogg"
        );
        assert!(upload(&dir, "ui", "Beep", &ogg, false).is_err());
        assert!(upload(&dir, "..", "x", &ogg, false).is_err());
        assert!(
            upload(&dir, "ui", "wav", b"RIFF....WAVE", false)
                .unwrap_err()
                .contains("OGG")
        );
        assert!(read(&dir, "sounds/ui/beep.ogg").is_some());
        assert!(read(&dir, "../Cargo.toml").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_checks_the_file_first() {
        let dir = tmp();
        let mut c = SoundConfig::default();
        c.events.insert(
            "no_such".into(),
            defs::EventSounds {
                sounds: vec![],
                volume: 10,
                priority: 0,
                positional: false,
            },
        );
        assert!(save(&dir, &c).unwrap_err().contains("no such event"));
        c.events.clear();
        save(&dir, &c).unwrap();
        let (back, _) = defs::read_sounds(&dir).unwrap();
        assert_eq!(back, c);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
