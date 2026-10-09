//! Client sound: one interface, our own mixer, a backend per platform with
//! no system library at build time.
//!
//! The mixer lives on the
//! backend's audio thread; [`Audio`] only sends it orders, so a slow frame
//! never cuts the sound. With no sound server (or no backend for the
//! platform yet) everything still works, silently.

mod bindings;
mod mixer;
#[cfg(target_os = "linux")]
mod pulse;
mod settings;
#[cfg(windows)]
mod wasapi_out;

pub use base::SoundEvent;
pub use bindings::{Binding, Bindings, ScreenTrack, SoundEntry, ZoneTrack, edge_fade, place};
pub use mixer::{MAX_VOICES, MUSIC_RAMP_FRAMES, Mixer, RATE, Volumes};
pub use settings::Settings;

use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

/// Where a sound goes; each has its own volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Channel {
    Effects,
    Ambience,
    Music,
    Interface,
}

/// How one sound plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Play {
    pub channel: Channel,
    /// 0 to 1: the event's volume times the distance.
    pub gain: f32,
    /// -1 (left) to 1 (right).
    pub pan: f32,
    /// With every voice busy, the lowest goes first.
    pub priority: u8,
}

/// A decoded sound: stereo frames at [`RATE`], shared without copying.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound(Arc<[[f32; 2]]>);

impl Sound {
    pub fn from_frames(frames: Vec<[f32; 2]>) -> Sound {
        Sound(frames.into())
    }

    pub fn frames(&self) -> &[[f32; 2]] {
        &self.0
    }

    /// Decodes an OGG Vorbis file, mono or stereo, at 44 100 Hz.
    pub fn decode_ogg(bytes: &[u8]) -> Result<Sound, String> {
        let mut r = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(bytes))
            .map_err(|e| format!("not an OGG Vorbis file: {e}"))?;
        let rate = r.ident_hdr.audio_sample_rate;
        if rate != RATE {
            return Err(format!("{rate} Hz; sounds must be {RATE} Hz"));
        }
        let channels = usize::from(r.ident_hdr.audio_channels);
        if !(1..=2).contains(&channels) {
            return Err(format!("{channels} channels; sounds are mono or stereo"));
        }
        let mut frames = Vec::new();
        loop {
            match r.read_dec_packet_itl() {
                Ok(Some(samples)) => {
                    for f in samples.chunks_exact(channels) {
                        let l = f32::from(f.first().copied().unwrap_or(0)) / 32768.0;
                        let r = f32::from(f.last().copied().unwrap_or(0)) / 32768.0;
                        frames.push([l, r]);
                    }
                }
                Ok(None) => break,
                Err(e) => return Err(format!("bad OGG Vorbis data: {e}")),
            }
        }
        Ok(Sound::from_frames(frames))
    }
}

/// An order for the audio thread.
pub(crate) enum Order {
    Play { key: u32, sound: Sound, play: Play },
    Volumes(Volumes),
    Music { track: Option<(u32, Sound)>, gain: f32 },
}

/// The sound of the client. Load each file once with [`Audio::load`], then
/// play it by the id it returns.
pub struct Audio {
    to_mixer: Option<Sender<Order>>,
    /// Keeps the backend's stream open.
    _backend: Option<Box<dyn std::any::Any>>,
    sounds: Vec<Sound>,
    /// The last music order sent, so a frame that changes nothing sends
    /// nothing.
    music: Option<(Option<SoundId>, f32)>,
}

/// A sound loaded into [`Audio`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(u32);

impl Audio {
    /// Opens the platform's backend under the application name `app` (what
    /// the system's sound mixer shows); with none, the client stays silent
    /// and the reason is returned for the log.
    pub fn new(app: &str) -> (Audio, Option<String>) {
        let (tx, rx) = channel();
        match backend(rx, app) {
            Ok(b) => (
                Audio {
                    to_mixer: Some(tx),
                    _backend: Some(b),
                    sounds: Vec::new(),
                    music: None,
                },
                None,
            ),
            Err(why) => (Audio::silent(), Some(why)),
        }
    }

    /// No backend at all: tests, and a client with no sound server.
    pub fn silent() -> Audio {
        Audio {
            to_mixer: None,
            _backend: None,
            sounds: Vec::new(),
            music: None,
        }
    }

    /// True when something can be heard.
    pub fn live(&self) -> bool {
        self.to_mixer.is_some()
    }

    pub fn load(&mut self, ogg: &[u8]) -> Result<SoundId, String> {
        let sound = Sound::decode_ogg(ogg)?;
        self.sounds.push(sound);
        Ok(SoundId((self.sounds.len() - 1) as u32))
    }

    pub fn play(&mut self, id: SoundId, play: Play) {
        if let (Some(tx), Some(sound)) = (&self.to_mixer, self.sounds.get(id.0 as usize)) {
            let order = Order::Play {
                key: id.0,
                sound: sound.clone(),
                play,
            };
            if tx.send(order).is_err() {
                self.to_mixer = None;
            }
        }
    }

    /// Loops `track` on the Music channel at `gain` (0 to 1); a different
    /// track crosses over in half a second, `None` fades the music out.
    pub fn music(&mut self, track: Option<SoundId>, gain: f32) {
        let gain = if track.is_some() {
            gain.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if let Some((t, g)) = self.music
            && t == track
            && (g - gain).abs() < 0.005
        {
            return;
        }
        self.music = Some((track, gain));
        let Some(tx) = &self.to_mixer else { return };
        let track = track.and_then(|id| Some((id.0, self.sounds.get(id.0 as usize)?.clone())));
        if tx.send(Order::Music { track, gain }).is_err() {
            self.to_mixer = None;
        }
    }

    pub fn set_settings(&mut self, s: &Settings) {
        if let Some(tx) = &self.to_mixer
            && tx.send(Order::Volumes(s.volumes())).is_err()
        {
            self.to_mixer = None;
        }
    }
}

#[cfg(target_os = "linux")]
fn backend(rx: std::sync::mpsc::Receiver<Order>, app: &str) -> Result<Box<dyn std::any::Any>, String> {
    pulse::open(rx, app)
}

#[cfg(windows)]
fn backend(rx: std::sync::mpsc::Receiver<Order>, _app: &str) -> Result<Box<dyn std::any::Any>, String> {
    wasapi_out::open(rx)
}

/// Other platforms (macOS) have no backend yet: silent.
#[cfg(not(any(target_os = "linux", windows)))]
fn backend(_rx: std::sync::mpsc::Receiver<Order>) -> Result<Box<dyn std::any::Any>, String> {
    Err("no sound backend for this platform yet".into())
}

/// Runs the orders that arrived, then fills `out`: what the audio thread
/// does each time the backend asks for frames.
pub(crate) fn feed(mixer: &mut Mixer, orders: &std::sync::mpsc::Receiver<Order>, out: &mut [f32]) {
    while let Ok(order) = orders.try_recv() {
        match order {
            Order::Play { key, sound, play } => {
                let _ = mixer.play(key, sound, play);
            }
            Order::Volumes(v) => mixer.set_volumes(v),
            Order::Music { track, gain } => mixer.set_music(track, gain),
        }
    }
    mixer.render(out);
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn silent_audio_takes_orders_and_does_nothing() {
        let mut a = Audio::silent();
        assert!(!a.live());
        assert!(a.load(b"not ogg").is_err());
        a.set_settings(&Settings::default());
    }

    #[test]
    fn orders_reach_the_mixer() {
        let (tx, rx) = channel();
        let mut m = Mixer::new();
        let sound = Sound::from_frames(vec![[0.5, 0.5]; 4]);
        let play = Play {
            channel: Channel::Effects,
            gain: 1.0,
            pan: 0.0,
            priority: 0,
        };
        tx.send(Order::Play { key: 1, sound, play }).unwrap();
        let mut out = [0.0; 4];
        feed(&mut m, &rx, &mut out);
        assert!(out[0] > 0.4);
    }

    /// The real backend opens and takes a sound; checked by hand on a
    /// machine with a sound server: `cargo test -p client-audio -- --ignored`.
    #[test]
    #[ignore = "needs a sound server"]
    fn the_backend_opens_and_plays() {
        let (mut a, why) = Audio::new("test");
        assert!(a.live(), "{why:?}");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/assets");
        let id = a
            .load(&std::fs::read(root.join("sounds/system/notice_1.ogg")).unwrap())
            .unwrap();
        a.set_settings(&Settings::default());
        a.play(
            id,
            Play {
                channel: Channel::Interface,
                gain: 0.5,
                pan: 0.0,
                priority: 0,
            },
        );
        std::thread::sleep(std::time::Duration::from_millis(600));
        assert!(a.live());
    }

    /// Every sound the project ships decodes (run after `examples/riftara/art-src/generate_sound.py`).
    #[test]
    fn the_shipped_sounds_decode() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/assets");
        let text = std::fs::read_to_string(root.join("sounds/sounds.ron")).unwrap_or_else(|_| "()".into());
        let b = Bindings::parse(&text).unwrap();
        for (name, entry) in &b.sounds {
            let bytes = std::fs::read(root.join(&entry.file)).unwrap();
            let s = Sound::decode_ogg(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!s.frames().is_empty(), "{name}");
        }
    }
}
