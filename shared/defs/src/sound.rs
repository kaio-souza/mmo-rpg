//! Sound bindings: which file plays for each
//! event the client fires. Events are code ([`SoundEvent`]); the binding is
//! data, `assets/sounds/sounds.ron`, edited on the editor's Sounds screen.
//!
//! ```ron
//! (
//!     sounds: {
//!         "hit_1": (file: "sounds/combat/hit_1.ogg", license: "CC0", credit: "Example"),
//!     },
//!     events: {
//!         "combat_hit": (sounds: ["hit_1"], volume: 800, priority: 100, positional: true),
//!     },
//! )
//! ```
//!
//! Adds `techniques` (technique id -> the same options as an
//! event; a technique without a line plays `technique_use`) and `zones`
//! (zone name -> looping music that fades out toward the zone's edges).
//!
//! P05 adds `screens` (a screen outside the world -> its music): `login`
//! plays on the login, selection and creation screens.
//!
//! An event with no line is silent. A file that is not on disk is a
//! warning, like the rest of the assets; anything else wrong is an error.

use crate::npc::valid_id;
pub use base::SoundEvent;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Sample rate every sound file must have.
pub const SOUND_SAMPLE_RATE: u32 = 44_100;
/// Most variations one event picks from.
pub const MAX_SOUND_VARIANTS: usize = 8;
/// Volume is per mille: 1000 plays the file as it is.
pub const FULL_VOLUME: u16 = 1000;
/// Tiles from a zone's edge to full music volume when `fade` is left out:
/// the width of the seam band.
pub const DEFAULT_MUSIC_FADE: u16 = 24;
/// Screens outside the world that may have music (P05): the client knows
/// these names.
pub const MUSIC_SCREENS: [&str; 1] = ["login"];
/// Licenses a sound may have.
pub const SOUND_LICENSES: [&str; 4] = ["CC0", "CC-BY-3.0", "CC-BY-4.0", "commissioned"];

/// One sound file and where it came from.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SoundFile {
    /// Path under `assets/`: `sounds/<folder>/<name>.ogg` or `music/<name>.ogg`.
    pub file: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub credit: String,
}

/// What plays for one event.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventSounds {
    /// Variations, one picked each time; empty is silent.
    #[serde(default)]
    pub sounds: Vec<String>,
    /// Per mille, 0 to 1000.
    #[serde(default = "full_volume")]
    pub volume: u16,
    /// 0 to 255: with every voice busy, the lowest goes first.
    #[serde(default)]
    pub priority: u8,
    /// Louder near the camera and to its side; ignored for interface events.
    #[serde(default)]
    pub positional: bool,
}

fn full_volume() -> u16 {
    FULL_VOLUME
}

fn default_fade() -> u16 {
    DEFAULT_MUSIC_FADE
}

/// The music of one zone.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneMusic {
    /// A name of `sounds`; plays in a loop.
    pub music: String,
    /// Per mille, 0 to 1000.
    #[serde(default = "full_volume")]
    pub volume: u16,
    /// Tiles from the zone's edge to full volume (0: no fade).
    #[serde(default = "default_fade")]
    pub fade: u16,
}

/// The music of a screen outside the world (P05).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenMusic {
    /// A name of `sounds`; plays in a loop.
    pub music: String,
    /// Per mille, 0 to 1000.
    #[serde(default = "full_volume")]
    pub volume: u16,
}

/// `assets/sounds/sounds.ron`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SoundConfig {
    #[serde(default)]
    pub sounds: BTreeMap<String, SoundFile>,
    #[serde(default)]
    pub events: BTreeMap<String, EventSounds>,
    /// Technique id -> its sound.
    #[serde(default)]
    pub techniques: BTreeMap<String, EventSounds>,
    /// Zone name -> its music.
    #[serde(default)]
    pub zones: BTreeMap<String, ZoneMusic>,
    /// Screen name ([`MUSIC_SCREENS`]) -> its music (P05).
    #[serde(default)]
    pub screens: BTreeMap<String, ScreenMusic>,
}

impl SoundConfig {
    /// Reads the text of a `sounds.ron`; checks nothing but the format.
    pub fn parse(text: &str) -> Result<SoundConfig, String> {
        ron::from_str(text).map_err(|e| format!("sounds.ron: {e}"))
    }

    /// What plays for `e`, if anything.
    pub fn event(&self, e: SoundEvent) -> Option<&EventSounds> {
        self.events.get(e.id()).filter(|s| !s.sounds.is_empty())
    }

    /// Every error; empty when the file may be written.
    pub fn check(&self) -> Vec<String> {
        let mut errs = Vec::new();
        for (name, s) in &self.sounds {
            let at = format!("sound `{name}`");
            if !valid_id(name) {
                errs.push(format!("{at}: the name must be [a-z0-9_]"));
            }
            if !valid_file(&s.file) {
                errs.push(format!(
                    "{at}: the file must be sounds/<folder>/<name>.ogg or music/<name>.ogg"
                ));
            }
            if !SOUND_LICENSES.contains(&s.license.as_str()) {
                errs.push(format!(
                    "{at}: the license must be one of {}",
                    SOUND_LICENSES.join(", ")
                ));
            }
            if s.credit.trim().is_empty() {
                errs.push(format!("{at}: the credit (author) is missing"));
            }
        }
        for id in self.events.keys() {
            if SoundEvent::from_id(id).is_none() {
                errs.push(format!("event `{id}`: no such event"));
            }
        }
        let bound = self
            .events
            .iter()
            .map(|(id, e)| (format!("event `{id}`"), e))
            .chain(
                self.techniques
                    .iter()
                    .map(|(id, e)| (format!("technique `{id}`"), e)),
            );
        for (at, e) in bound {
            if e.volume > FULL_VOLUME {
                errs.push(format!("{at}: volume goes from 0 to {FULL_VOLUME}"));
            }
            if e.sounds.len() > MAX_SOUND_VARIANTS {
                errs.push(format!("{at}: at most {MAX_SOUND_VARIANTS} sounds"));
            }
            for s in e.sounds.iter().filter(|s| !self.sounds.contains_key(*s)) {
                errs.push(format!("{at}: no sound `{s}`"));
            }
        }
        let music = self
            .zones
            .iter()
            .map(|(zone, m)| (format!("zone `{zone}`"), &m.music, m.volume))
            .chain(
                self.screens
                    .iter()
                    .map(|(screen, m)| (format!("screen `{screen}`"), &m.music, m.volume)),
            );
        for (at, music, volume) in music {
            if volume > FULL_VOLUME {
                errs.push(format!("{at}: volume goes from 0 to {FULL_VOLUME}"));
            }
            if !self.sounds.contains_key(music) {
                errs.push(format!("{at}: no sound `{music}`"));
            }
        }
        for screen in self
            .screens
            .keys()
            .filter(|s| !MUSIC_SCREENS.contains(&s.as_str()))
        {
            errs.push(format!(
                "screen `{screen}`: no such screen (one of {})",
                MUSIC_SCREENS.join(", ")
            ));
        }
        errs
    }

    /// Errors of technique ids and zone names that the content does not
    /// have (`validate` with the catalog; the editor offers only real ones).
    pub fn check_refs<'a>(
        &self,
        techniques: impl IntoIterator<Item = &'a str>,
        zones: impl IntoIterator<Item = &'a str>,
    ) -> Vec<String> {
        let techniques: BTreeSet<&str> = techniques.into_iter().collect();
        let zones: BTreeSet<&str> = zones.into_iter().collect();
        let mut errs: Vec<String> = self
            .techniques
            .keys()
            .filter(|t| !techniques.contains(t.as_str()))
            .map(|t| format!("technique `{t}`: no such technique in content/"))
            .collect();
        errs.extend(
            self.zones
                .keys()
                .filter(|z| !zones.contains(z.as_str()))
                .map(|z| format!("zone `{z}`: no such zone in content/zones/")),
        );
        errs
    }

    /// One warning per sound whose file is not among `files` (paths under
    /// `assets/`, as [`SoundFile::file`] writes them).
    pub fn missing_files(&self, files: &BTreeSet<String>) -> Vec<String> {
        self.sounds
            .iter()
            .filter(|(_, s)| !files.contains(&s.file))
            .map(|(n, s)| format!("warning: sound `{n}`: no file assets/{}", s.file))
            .collect()
    }

    /// The file in a fixed layout, one entry per line, so a change reads
    /// well in a diff.
    pub fn to_ron(&self) -> String {
        let mut out = String::from(
            "// Which sound plays for each event; edit on the editor's Sounds screen.\n(\n    sounds: {\n",
        );
        for (name, s) in &self.sounds {
            let _ = writeln!(
                out,
                "        {}: (file: {}, license: {}, credit: {}),",
                quote(name),
                quote(&s.file),
                quote(&s.license),
                quote(&s.credit)
            );
        }
        out.push_str("    },\n    events: {\n");
        write_bound(&mut out, &self.events);
        out.push_str("    },\n    techniques: {\n");
        write_bound(&mut out, &self.techniques);
        out.push_str("    },\n    zones: {\n");
        for (zone, m) in &self.zones {
            let _ = writeln!(
                out,
                "        {}: (music: {}, volume: {}, fade: {}),",
                quote(zone),
                quote(&m.music),
                m.volume,
                m.fade
            );
        }
        out.push_str("    },\n    screens: {\n");
        for (screen, m) in &self.screens {
            let _ = writeln!(
                out,
                "        {}: (music: {}, volume: {}),",
                quote(screen),
                quote(&m.music),
                m.volume
            );
        }
        out.push_str("    },\n)\n");
        out
    }
}

fn write_bound(out: &mut String, bound: &BTreeMap<String, EventSounds>) {
    for (id, e) in bound {
        let list: Vec<String> = e.sounds.iter().map(|s| quote(s)).collect();
        let _ = writeln!(
            out,
            "        {}: (sounds: [{}], volume: {}, priority: {}, positional: {}),",
            quote(id),
            list.join(", "),
            e.volume,
            e.priority,
            e.positional
        );
    }
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `sounds/<folder>/<name>.ogg` or `music/<name>.ogg`, names in [a-z0-9_].
pub fn valid_file(path: &str) -> bool {
    let Some(stem) = path.strip_suffix(".ogg") else {
        return false;
    };
    let parts: Vec<&str> = stem.split('/').collect();
    match parts.as_slice() {
        ["sounds", folder, name] => valid_id(folder) && valid_id(name),
        ["music", name] => valid_id(name),
        _ => false,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn sample() -> SoundConfig {
        SoundConfig::parse(
            r#"(
                sounds: { "hit_1": (file: "sounds/combat/hit_1.ogg", license: "CC0", credit: "Example") },
                events: { "combat_hit": (sounds: ["hit_1"], volume: 800, priority: 100, positional: true) },
                techniques: { "cleave": (sounds: ["hit_1"]) },
                zones: { "aurelia": (music: "hit_1", volume: 500) },
                screens: { "login": (music: "hit_1", volume: 600) },
            )"#,
        )
        .unwrap()
    }

    #[test]
    fn a_good_file_has_no_problem_and_survives_its_own_layout() {
        let c = sample();
        assert_eq!(c.check(), Vec::<String>::new());
        assert_eq!(SoundConfig::parse(&c.to_ron()).unwrap(), c);
        assert_eq!(c.event(SoundEvent::CombatHit).unwrap().volume, 800);
        assert!(c.event(SoundEvent::UiClick).is_none());
        assert_eq!(c.zones["aurelia"].fade, DEFAULT_MUSIC_FADE);
        assert_eq!(c.screens["login"].volume, 600);
        let mut odd = c.clone();
        odd.screens.insert("title".into(), odd.screens["login"].clone());
        assert!(
            odd.check().iter().any(|e| e.contains("screen `title`")),
            "unknown screen"
        );
        assert!(c.check_refs(["cleave"], ["aurelia"]).is_empty());
        let errs = c.check_refs([], []).join("\n");
        assert!(
            errs.contains("technique `cleave`") && errs.contains("zone `aurelia`"),
            "{errs}"
        );
    }

    #[test]
    fn every_event_id_goes_both_ways() {
        for e in SoundEvent::ALL {
            assert_eq!(SoundEvent::from_id(e.id()), Some(e));
        }
        assert!(SoundEvent::UiClick.interface() && !SoundEvent::CombatHit.interface());
    }

    #[test]
    fn wrong_names_files_licenses_and_references_are_errors() {
        let mut c = sample();
        c.sounds.insert(
            "Bad".into(),
            SoundFile {
                file: "../x.wav".into(),
                license: "GPL".into(),
                credit: String::new(),
            },
        );
        c.events.insert(
            "no_such".into(),
            EventSounds {
                sounds: vec!["ghost".into()],
                volume: 1001,
                priority: 0,
                positional: false,
            },
        );
        c.zones.insert(
            "fenmoor".into(),
            ZoneMusic {
                music: "phantom".into(),
                volume: 10,
                fade: 5,
            },
        );
        let errs = c.check().join("\n");
        for part in [
            "[a-z0-9_]",
            "sounds/<folder>",
            "license",
            "credit",
            "no such event",
            "volume",
            "no sound `ghost`",
            "no sound `phantom`",
        ] {
            assert!(errs.contains(part), "{part} not in {errs}");
        }
    }

    #[test]
    fn a_missing_file_is_a_warning() {
        let c = sample();
        assert_eq!(c.missing_files(&BTreeSet::new()).len(), 1);
        let files = BTreeSet::from(["sounds/combat/hit_1.ogg".to_owned()]);
        assert!(c.missing_files(&files).is_empty());
    }

    /// The shipped file is clean and already in the editor's layout, so the
    /// first save from the editor changes nothing.
    #[test]
    #[allow(clippy::disallowed_methods)]
    fn the_shipped_file_is_valid_and_in_the_fixed_layout() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/assets");
        let (config, files) = crate::read_sounds(&root).unwrap();
        assert_eq!(config.check(), Vec::<String>::new());
        assert_eq!(config.missing_files(&files), Vec::<String>::new());
        let text = std::fs::read_to_string(root.join("sounds/sounds.ron")).unwrap();
        assert_eq!(config.to_ron(), text);
    }

    #[test]
    fn file_paths() {
        assert!(valid_file("sounds/ui/click.ogg") && valid_file("music/title.ogg"));
        for bad in [
            "sounds/click.ogg",
            "sounds/ui/../x.ogg",
            "music/a/b.ogg",
            "sounds/ui/click.wav",
            "/abs.ogg",
        ] {
            assert!(!valid_file(bad), "{bad}");
        }
    }
}
