//! The client's sound: which file plays for each event
//! (`assets/sounds/sounds.ron`, edited on the editor's Sounds screen) and
//! the player's volumes (`sound.ron` in the user folder). Like the art, the
//! bindings are read again a second after the editor saves them.:
//! each technique may have its own sound, and each zone its music, which
//! fades out toward the zone's edges.

use crate::interface::config_dir;
use client_audio::{Audio, Binding, Bindings, Channel, Play, Settings, SoundEvent, SoundId};
use client_ui::{SoundModel, VolumeSlot};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use tracing::warn;

/// How often `sounds.ron` is looked at for a change.
const POLL: Duration = Duration::from_secs(1);
/// Heard up to this many tiles when the server has not said the sight yet.
const DEFAULT_RADIUS: f32 = 20.0;

pub struct Sounds {
    audio: Audio,
    assets: PathBuf,
    bindings: Bindings,
    loaded: BTreeMap<String, SoundId>,
    /// Which variation of each event plays next.
    turn: u32,
    pub settings: Settings,
    seen: Option<SystemTime>,
    polled: Instant,
}

impl Sounds {
    /// No sound at all (before the window opens, and in tests).
    pub fn silent() -> Sounds {
        Sounds {
            audio: Audio::silent(),
            assets: PathBuf::new(),
            bindings: Bindings::default(),
            loaded: BTreeMap::new(),
            turn: 0,
            settings: Settings::default(),
            seen: None,
            polled: Instant::now(),
        }
    }

    /// Opens the sound server and loads the sounds the events use.
    pub fn open(assets: &Path) -> Sounds {
        let (audio, why) = Audio::new(client_ui::GAME_ID);
        if let Some(why) = why {
            warn!("sound off: {why}");
        }
        let mut s = Sounds {
            audio,
            assets: assets.to_path_buf(),
            ..Sounds::silent()
        };
        s.settings = load_settings();
        s.audio.set_settings(&s.settings);
        s.reload();
        s
    }

    fn bindings_path(&self) -> PathBuf {
        self.assets.join("sounds").join("sounds.ron")
    }

    /// Reads `sounds.ron` again and loads every file an event uses.
    fn reload(&mut self) {
        let path = self.bindings_path();
        self.seen = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        self.bindings = match std::fs::read_to_string(&path) {
            Ok(text) => Bindings::parse(&text).unwrap_or_else(|e| {
                warn!("{e}");
                Bindings::default()
            }),
            Err(_) => Bindings::default(),
        };
        if !self.audio.live() {
            return;
        }
        let b = &self.bindings;
        let used: Vec<String> = b
            .events
            .values()
            .chain(b.techniques.values())
            .flat_map(|b| b.sounds.iter().cloned())
            .chain(b.zones.values().map(|z| z.music.clone()))
            .chain(b.screens.values().map(|s| s.music.clone()))
            .collect();
        // A file changed on disk keeps its name: load everything again.
        self.loaded.clear();
        for name in used {
            if self.loaded.contains_key(&name) {
                continue;
            }
            let Some(entry) = self.bindings.sounds.get(&name) else {
                continue;
            };
            let file = self.assets.join(&entry.file);
            match std::fs::read(&file)
                .map_err(|e| e.to_string())
                .and_then(|b| self.audio.load(&b))
            {
                Ok(id) => {
                    self.loaded.insert(name, id);
                }
                Err(e) => warn!(file = %file.display(), "sound not loaded: {e}"),
            }
        }
    }

    /// Once a second: `sounds.ron` saved by the editor plays at once.
    pub fn poll(&mut self) {
        if self.polled.elapsed() < POLL {
            return;
        }
        self.polled = Instant::now();
        let now = std::fs::metadata(self.bindings_path())
            .and_then(|m| m.modified())
            .ok();
        if now != self.seen {
            self.reload();
        }
    }

    /// Plays the sound of `e`; `at` is where it happens, in tiles from the
    /// player, for the events placed in the world.
    pub fn play(&mut self, e: SoundEvent, at: Option<(f32, f32)>, radius: u16) {
        if let Some(b) = self.bindings.get(e).cloned() {
            self.play_binding(&b, e.interface(), at, radius);
        }
    }

    /// The sound of the technique `id` (its own line, else `technique_use`).
    pub fn technique(&mut self, id: &str, at: Option<(f32, f32)>, radius: u16) {
        if let Some(b) = self.bindings.technique(id).cloned() {
            self.play_binding(&b, false, at, radius);
        }
    }

    /// Each frame: the music of the zone the player stands in (`zone`, and
    /// how many tiles from its nearest edge), quieter toward the edge.
    pub fn music(&mut self, here: Option<(&str, i32)>) {
        let track = here.and_then(|(zone, edge)| {
            let z = self.bindings.zones.get(zone)?;
            let id = *self.loaded.get(&z.music)?;
            Some((
                id,
                f32::from(z.volume) / 1000.0 * client_audio::edge_fade(edge, z.fade),
            ))
        });
        match track {
            Some((id, gain)) => self.audio.music(Some(id), gain),
            None => self.audio.music(None, 0.0),
        }
    }

    /// Each frame outside the world: the music of `screen` (P05, `login`
    /// for the login, selection and creation screens); none is silence.
    pub fn screen_music(&mut self, screen: &str) {
        let track = self
            .bindings
            .screens
            .get(screen)
            .and_then(|s| Some((*self.loaded.get(&s.music)?, f32::from(s.volume) / 1000.0)));
        match track {
            Some((id, gain)) => self.audio.music(Some(id), gain),
            None => self.audio.music(None, 0.0),
        }
    }

    fn play_binding(&mut self, b: &Binding, interface: bool, at: Option<(f32, f32)>, radius: u16) {
        let Some(name) = b.sounds.get(self.turn as usize % b.sounds.len()) else {
            return;
        };
        self.turn = self.turn.wrapping_add(1);
        let Some(&id) = self.loaded.get(name) else {
            return;
        };
        let radius = if radius == 0 {
            DEFAULT_RADIUS
        } else {
            f32::from(radius)
        };
        let (near, pan) = match at {
            Some((dx, dy)) if b.positional && !interface => client_audio::place(dx, dy, radius),
            _ => (1.0, 0.0),
        };
        if near <= 0.0 {
            return;
        }
        let channel = if interface {
            Channel::Interface
        } else {
            Channel::Effects
        };
        self.audio.play(
            id,
            Play {
                channel,
                gain: f32::from(b.volume) / 1000.0 * near,
                pan,
                priority: b.priority,
            },
        );
    }

    /// The Options window's view of the settings.
    pub fn model(&self) -> SoundModel {
        SoundModel {
            volumes: VolumeSlot::ALL
                .iter()
                .map(|&v| (v, self.settings.get(channel_of(v))))
                .collect(),
            muted: self.settings.muted,
            live: self.audio.live(),
        }
    }

    pub fn step(&mut self, slot: VolumeSlot, up: bool) {
        self.settings.step(channel_of(slot), up);
        self.changed();
    }

    pub fn toggle_mute(&mut self) {
        self.settings.muted = !self.settings.muted;
        self.changed();
    }

    fn changed(&mut self) {
        self.audio.set_settings(&self.settings);
        save_settings(&self.settings);
    }
}

fn channel_of(v: VolumeSlot) -> Option<Channel> {
    match v {
        VolumeSlot::Master => None,
        VolumeSlot::Effects => Some(Channel::Effects),
        VolumeSlot::Ambience => Some(Channel::Ambience),
        VolumeSlot::Music => Some(Channel::Music),
        VolumeSlot::Interface => Some(Channel::Interface),
    }
}

fn load_settings() -> Settings {
    let Some(path) = config_dir().map(|d| d.join("sound.ron")) else {
        return Settings::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => Settings::parse(&text).unwrap_or_else(|e| {
            warn!(path = %path.display(), "{e}");
            Settings::default()
        }),
        Err(_) => Settings::default(),
    }
}

fn save_settings(s: &Settings) {
    let Some(dir) = config_dir() else { return };
    let written =
        std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join("sound.ron"), s.to_ron()));
    if let Err(e) = written {
        warn!(dir = %dir.display(), "cannot save sound.ron: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_silent_client_plays_nothing_and_shows_every_volume() {
        let mut s = Sounds::silent();
        s.play(SoundEvent::UiClick, None, 0);
        s.technique("cleave", None, 0);
        s.music(Some(("aurelia", 3)));
        let m = s.model();
        assert_eq!(m.volumes.len(), 5);
        assert!(!m.live);
        assert_eq!(channel_of(VolumeSlot::Master), None);
    }
}
