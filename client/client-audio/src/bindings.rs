//! `assets/sounds/sounds.ron` as the client reads it: which file plays for
//! each event. `defs::SoundConfig` is the checked copy of the same format
//! (`validate` and the editor); this one keeps only what playing needs and
//! skips license and credit.

use base::SoundEvent;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Bindings {
    #[serde(default)]
    pub sounds: BTreeMap<String, SoundEntry>,
    #[serde(default)]
    pub events: BTreeMap<String, Binding>,
    /// Technique id -> its sound.
    #[serde(default)]
    pub techniques: BTreeMap<String, Binding>,
    /// Zone name -> its music.
    #[serde(default)]
    pub zones: BTreeMap<String, ZoneTrack>,
    /// Screen outside the world (`login`) -> its music (P05).
    #[serde(default)]
    pub screens: BTreeMap<String, ScreenTrack>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ScreenTrack {
    /// A name of `sounds`.
    pub music: String,
    #[serde(default = "full")]
    pub volume: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ZoneTrack {
    /// A name of `sounds`.
    pub music: String,
    #[serde(default = "full")]
    pub volume: u16,
    /// Tiles from the edge to full volume.
    #[serde(default = "seam")]
    pub fade: u16,
}

fn seam() -> u16 {
    24
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SoundEntry {
    /// Path under `assets/`.
    pub file: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Binding {
    #[serde(default)]
    pub sounds: Vec<String>,
    /// Per mille.
    #[serde(default = "full")]
    pub volume: u16,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub positional: bool,
}

fn full() -> u16 {
    1000
}

impl Bindings {
    pub fn parse(text: &str) -> Result<Bindings, String> {
        ron::from_str(text).map_err(|e| format!("sounds.ron: {e}"))
    }

    /// What plays for `e`; `None` is silence.
    pub fn get(&self, e: SoundEvent) -> Option<&Binding> {
        self.events.get(e.id()).filter(|b| !b.sounds.is_empty())
    }

    /// What plays for the technique `id`: its own line, else
    /// `technique_use`.
    pub fn technique(&self, id: &str) -> Option<&Binding> {
        self.techniques
            .get(id)
            .filter(|b| !b.sounds.is_empty())
            .or_else(|| self.get(SoundEvent::TechniqueUse))
    }
}

/// How loud zone music is `edge` tiles in from the zone's nearest edge:
/// 0 on the edge, 1 from `fade` tiles in, a smooth curve between.
pub fn edge_fade(edge: i32, fade: u16) -> f32 {
    if fade == 0 {
        return 1.0;
    }
    let t = (edge.max(0) as f32 / f32::from(fade)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Gain and pan of a sound `(dx, dy)` tiles from the listener, heard up to
/// `radius` tiles. The screen is isometric: east and north lean right.
pub fn place(dx: f32, dy: f32, radius: f32) -> (f32, f32) {
    let r = radius.max(1.0);
    let gain = (1.0 - (dx * dx + dy * dy).sqrt() / r).clamp(0.0, 1.0);
    let pan = ((dx - dy) / r).clamp(-1.0, 1.0);
    (gain, pan)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_format_and_ignores_the_credits() {
        let b = Bindings::parse(
            r#"(sounds: {"a": (file: "sounds/ui/a.ogg", license: "CC0", credit: "x")},
                events: {"ui_click": (sounds: ["a"], volume: 500), "ui_error": (sounds: [])})"#,
        )
        .unwrap();
        assert_eq!(b.get(SoundEvent::UiClick).unwrap().volume, 500);
        assert!(b.get(SoundEvent::UiError).is_none() && b.get(SoundEvent::CombatHit).is_none());
    }

    #[test]
    fn techniques_fall_back_and_zones_read_their_fade() {
        let b = Bindings::parse(
            r#"(sounds: {"a": (file: "sounds/ui/a.ogg")},
                events: {"technique_use": (sounds: ["a"], volume: 300)},
                techniques: {"cleave": (sounds: ["a"], volume: 900)},
                zones: {"aurelia": (music: "a")},
                screens: {"login": (music: "a", volume: 600)})"#,
        )
        .unwrap();
        assert_eq!(b.technique("cleave").unwrap().volume, 900);
        assert_eq!(b.technique("other").unwrap().volume, 300);
        assert_eq!(b.zones["aurelia"].fade, 24);
        assert_eq!(b.screens["login"].volume, 600);
        assert_eq!(edge_fade(0, 24), 0.0);
        assert_eq!(edge_fade(24, 24), 1.0);
        assert_eq!(edge_fade(100, 24), 1.0);
        assert!((edge_fade(12, 24) - 0.5).abs() < 1e-6);
        assert_eq!(edge_fade(0, 0), 1.0);
    }

    #[test]
    fn near_is_loud_far_is_silent_east_is_right() {
        assert_eq!(place(0.0, 0.0, 20.0), (1.0, 0.0));
        assert_eq!(place(30.0, 0.0, 20.0).0, 0.0);
        let (g, p) = place(5.0, 0.0, 20.0);
        assert!(g > 0.7 && p > 0.0);
        assert!(place(-5.0, 0.0, 20.0).1 < 0.0);
    }
}
