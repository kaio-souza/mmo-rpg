//! The mixer: sums the voices into one stereo buffer. Pure code, no
//! backend, so it is tested in memory.

use crate::{Channel, Play, Sound};

/// Most voices at once.
pub const MAX_VOICES: usize = 32;
/// Frames per second of every sound and of the output.
pub const RATE: u32 = 44_100;
/// The same sound asked for again within this many frames plays once (50 ms).
pub const REPEAT_FRAMES: u64 = RATE as u64 / 20;

struct Voice {
    sound: Sound,
    /// Next frame of `sound` to play.
    at: usize,
    channel: Channel,
    left: f32,
    right: f32,
    priority: u8,
    /// Loudness before the channel volume: what is far goes first.
    gain: f32,
}

/// Frames for the music gain to go all the way from 0 to 1 (half a
/// second): smooths the steps of the walk and crosses two tracks.
pub const MUSIC_RAMP_FRAMES: f32 = RATE as f32 / 2.0;

/// A music track looping on the Music channel.
struct Track {
    key: u32,
    sound: Sound,
    at: usize,
    gain: f32,
    target: f32,
}

/// Volume of each channel and of the whole, 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volumes {
    pub master: f32,
    pub effects: f32,
    pub ambience: f32,
    pub music: f32,
    pub interface: f32,
    pub muted: bool,
}

impl Default for Volumes {
    fn default() -> Self {
        Volumes {
            master: 1.0,
            effects: 1.0,
            ambience: 1.0,
            music: 1.0,
            interface: 1.0,
            muted: false,
        }
    }
}

impl Volumes {
    fn of(&self, c: Channel) -> f32 {
        if self.muted {
            return 0.0;
        }
        self.master
            * match c {
                Channel::Effects => self.effects,
                Channel::Ambience => self.ambience,
                Channel::Music => self.music,
                Channel::Interface => self.interface,
            }
    }
}

#[derive(Default)]
pub struct Mixer {
    voices: Vec<Voice>,
    volumes: Volumes,
    /// Frames played so far.
    frame: u64,
    /// When each sound key last started.
    started: Vec<(u32, u64)>,
    /// The music playing (last) and the tracks fading out before it.
    music: Vec<Track>,
}

impl Mixer {
    pub fn new() -> Mixer {
        Mixer::default()
    }

    pub fn set_volumes(&mut self, v: Volumes) {
        self.volumes = v;
    }

    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    /// The music to loop and its gain (0 to 1). Another `key` fades the
    /// track before it out while the new one fades in; `None` fades all out.
    pub fn set_music(&mut self, track: Option<(u32, Sound)>, gain: f32) {
        let gain = gain.clamp(0.0, 1.0);
        let Some((key, sound)) = track else {
            for t in &mut self.music {
                t.target = 0.0;
            }
            return;
        };
        if let Some(t) = self.music.last_mut()
            && t.key == key
        {
            t.target = gain;
            return;
        }
        for t in &mut self.music {
            t.target = 0.0;
        }
        // A track that was fading out comes back from where it is.
        let (at, from) = match self.music.iter().position(|t| t.key == key) {
            Some(i) => {
                let t = self.music.remove(i);
                (t.at, t.gain)
            }
            None => (0, 0.0),
        };
        self.music.push(Track {
            key,
            sound,
            at,
            gain: from,
            target: gain,
        });
    }

    /// Gain of the music track `key` right now (tests).
    pub fn music_gain(&self, key: u32) -> Option<f32> {
        self.music.iter().find(|t| t.key == key).map(|t| t.gain)
    }

    /// Starts `sound` (named by `key`); false when it did not start: asked
    /// again too soon, or every voice busy with something that matters more.
    pub fn play(&mut self, key: u32, sound: Sound, p: Play) -> bool {
        let now = self.frame;
        if self
            .started
            .iter()
            .any(|&(k, at)| k == key && now.saturating_sub(at) < REPEAT_FRAMES)
        {
            return false;
        }
        let gain = p.gain.clamp(0.0, 1.0);
        // Constant power: the center is as loud as either side.
        let angle = (p.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        let voice = Voice {
            sound,
            at: 0,
            channel: p.channel,
            left: gain * angle.cos() * std::f32::consts::SQRT_2,
            right: gain * angle.sin() * std::f32::consts::SQRT_2,
            priority: p.priority,
            gain,
        };
        if self.voices.len() >= MAX_VOICES {
            let weakest = self
                .voices
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    (a.priority, a.gain)
                        .partial_cmp(&(b.priority, b.gain))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, v)| (i, v.priority, v.gain));
            match weakest {
                Some((i, pr, g)) if (voice.priority, voice.gain) > (pr, g) => {
                    self.voices.swap_remove(i);
                }
                _ => return false,
            }
        }
        self.started
            .retain(|&(k, at)| k != key && now.saturating_sub(at) < REPEAT_FRAMES);
        self.started.push((key, now));
        self.voices.push(voice);
        true
    }

    /// Fills `out` (left, right, left, right...) with the next frames and
    /// drops the voices that ended.
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let frames = out.len() / 2;
        let volumes = self.volumes;
        for v in &mut self.voices {
            let k = volumes.of(v.channel);
            let (l, r) = (v.left * k, v.right * k);
            let rest = v.sound.frames().get(v.at..).unwrap_or_default();
            for (o, s) in out.chunks_exact_mut(2).zip(rest) {
                if let [ol, or] = o {
                    *ol += s[0] * l;
                    *or += s[1] * r;
                }
            }
            v.at += frames.min(rest.len());
        }
        self.voices.retain(|v| v.at < v.sound.frames().len());
        let k = volumes.of(Channel::Music);
        let step = 1.0 / MUSIC_RAMP_FRAMES;
        for t in &mut self.music {
            let len = t.sound.frames().len();
            if len == 0 {
                continue;
            }
            for o in out.chunks_exact_mut(2) {
                t.gain += (t.target - t.gain).clamp(-step, step);
                let f = t.sound.frames().get(t.at).copied().unwrap_or_default();
                if let [ol, or] = o {
                    *ol += f[0] * t.gain * k;
                    *or += f[1] * t.gain * k;
                }
                t.at = (t.at + 1) % len;
            }
        }
        self.music.retain(|t| t.target > 0.0 || t.gain > 0.0);
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
        self.frame += frames as u64;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn tone(frames: usize, v: f32) -> Sound {
        Sound::from_frames(vec![[v, v]; frames])
    }

    fn effect() -> Play {
        Play {
            channel: Channel::Effects,
            gain: 1.0,
            pan: 0.0,
            priority: 10,
        }
    }

    #[test]
    fn two_voices_add_and_end() {
        let mut m = Mixer::new();
        assert!(m.play(1, tone(4, 0.25), effect()));
        assert!(m.play(2, tone(2, 0.25), effect()));
        let mut out = [0.0; 8];
        m.render(&mut out);
        assert!((out[0] - 0.5).abs() < 1e-4, "{out:?}");
        assert!((out[6] - 0.25).abs() < 1e-4, "{out:?}");
        assert_eq!(m.voices(), 0);
    }

    #[test]
    fn channel_and_master_volume_and_mute() {
        let mut m = Mixer::new();
        m.set_volumes(Volumes {
            master: 0.5,
            effects: 0.5,
            ..Volumes::default()
        });
        m.play(1, tone(2, 1.0), effect());
        m.play(
            2,
            tone(2, 1.0),
            Play {
                channel: Channel::Interface,
                ..effect()
            },
        );
        let mut out = [0.0; 2];
        m.render(&mut out);
        assert!((out[0] - 0.75).abs() < 1e-4, "{out:?}");
        let mut m = Mixer::new();
        m.set_volumes(Volumes {
            muted: true,
            ..Volumes::default()
        });
        m.play(1, tone(2, 1.0), effect());
        m.render(&mut out);
        assert_eq!(out, [0.0, 0.0]);
    }

    #[test]
    fn pan_moves_the_sound_to_one_side() {
        let mut m = Mixer::new();
        m.play(
            1,
            tone(1, 0.5),
            Play {
                pan: -1.0,
                ..effect()
            },
        );
        let mut out = [0.0; 2];
        m.render(&mut out);
        assert!(out[0] > 0.6 && out[1].abs() < 1e-4, "{out:?}");
    }

    #[test]
    fn the_same_sound_twice_in_50_ms_plays_once() {
        let mut m = Mixer::new();
        assert!(m.play(7, tone(10_000, 0.1), effect()));
        assert!(!m.play(7, tone(10_000, 0.1), effect()));
        let mut out = vec![0.0; 2 * REPEAT_FRAMES as usize];
        m.render(&mut out);
        assert!(m.play(7, tone(10, 0.1), effect()));
    }

    #[test]
    fn music_loops_ramps_and_crosses_tracks() {
        let mut m = Mixer::new();
        m.set_music(Some((1, tone(3, 1.0))), 1.0);
        let mut out = vec![0.0; 2 * RATE as usize];
        m.render(&mut out);
        // It ramps up over half a second and keeps looping a 3-frame sound.
        assert!(
            out[0].abs() < 0.01 && (out[out.len() - 2] - 1.0).abs() < 1e-3,
            "{}",
            out[0]
        );
        assert_eq!(m.music_gain(1), Some(1.0));
        m.set_music(Some((2, tone(5, 0.5))), 0.5);
        m.render(&mut out[..2 * 1000]);
        let (a, b) = (m.music_gain(1).unwrap(), m.music_gain(2).unwrap());
        assert!(a < 1.0 && b > 0.0, "{a} {b}");
        m.render(&mut out);
        assert_eq!(m.music_gain(1), None);
        assert_eq!(m.music_gain(2), Some(0.5));
        m.set_music(None, 0.0);
        m.render(&mut out);
        assert_eq!(m.music_gain(2), None);
    }

    #[test]
    fn a_full_mixer_drops_the_weakest_voice() {
        let mut m = Mixer::new();
        for k in 0..MAX_VOICES as u32 {
            assert!(m.play(
                k,
                tone(100, 0.0),
                Play {
                    gain: 0.5,
                    ..effect()
                }
            ));
        }
        assert!(!m.play(
            100,
            tone(100, 0.0),
            Play {
                gain: 0.1,
                ..effect()
            }
        ));
        assert!(m.play(
            101,
            tone(100, 0.0),
            Play {
                priority: 200,
                ..effect()
            }
        ));
        assert_eq!(m.voices(), MAX_VOICES);
    }
}
