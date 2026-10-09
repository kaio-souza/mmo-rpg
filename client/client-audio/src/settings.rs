//! The player's sound settings, `sound.ron` in the user folder:
//! volume of the whole and of each channel, per mille, and mute. Not
//! content: each player keeps their own.

use crate::{Channel, Volumes};
use serde::{Deserialize, Serialize};

/// One step of the − and + buttons: 10%.
pub const STEP: u16 = 100;
const FULL: u16 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub master: u16,
    pub effects: u16,
    pub ambience: u16,
    pub music: u16,
    pub interface: u16,
    pub muted: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            master: 800,
            effects: FULL,
            ambience: 700,
            music: 600,
            interface: 800,
            muted: false,
        }
    }
}

impl Settings {
    /// Reads `sound.ron`; values above 100% are cut to it.
    pub fn parse(text: &str) -> Result<Settings, String> {
        let s: Settings = ron::from_str(text).map_err(|e| format!("sound.ron: {e}"))?;
        Ok(s.clamped())
    }

    pub fn to_ron(&self) -> String {
        format!(
            "// Sound settings: volumes per mille, 0 to 1000.\n(master: {}, effects: {}, ambience: {}, music: {}, interface: {}, muted: {})\n",
            self.master, self.effects, self.ambience, self.music, self.interface, self.muted
        )
    }

    fn clamped(mut self) -> Settings {
        for v in [
            &mut self.master,
            &mut self.effects,
            &mut self.ambience,
            &mut self.music,
            &mut self.interface,
        ] {
            *v = (*v).min(FULL);
        }
        self
    }

    /// The volume of `channel`, or of the whole with `None`.
    pub fn get(&self, channel: Option<Channel>) -> u16 {
        match channel {
            None => self.master,
            Some(Channel::Effects) => self.effects,
            Some(Channel::Ambience) => self.ambience,
            Some(Channel::Music) => self.music,
            Some(Channel::Interface) => self.interface,
        }
    }

    pub fn set(&mut self, channel: Option<Channel>, per_mille: u16) {
        let v = per_mille.min(FULL);
        match channel {
            None => self.master = v,
            Some(Channel::Effects) => self.effects = v,
            Some(Channel::Ambience) => self.ambience = v,
            Some(Channel::Music) => self.music = v,
            Some(Channel::Interface) => self.interface = v,
        }
    }

    /// One step up or down, kept on the 10% marks.
    pub fn step(&mut self, channel: Option<Channel>, up: bool) {
        let now = self.get(channel);
        let next = if up {
            (now / STEP + 1) * STEP
        } else {
            now.div_ceil(STEP).saturating_sub(1) * STEP
        };
        self.set(channel, next);
    }

    pub fn volumes(&self) -> Volumes {
        let k = |v: u16| f32::from(v) / f32::from(FULL);
        Volumes {
            master: k(self.master),
            effects: k(self.effects),
            ambience: k(self.ambience),
            music: k(self.music),
            interface: k(self.interface),
            muted: self.muted,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_file_goes_both_ways_and_is_cut_to_full() {
        let s = Settings {
            music: 300,
            muted: true,
            ..Settings::default()
        };
        assert_eq!(Settings::parse(&s.to_ron()).unwrap(), s);
        assert_eq!(Settings::parse("(master: 5000)").unwrap().master, 1000);
        assert_eq!(Settings::parse("()").unwrap(), Settings::default());
        assert!(Settings::parse("(loud: 1)").is_err());
    }

    #[test]
    fn steps_stay_on_the_marks_and_inside_the_range() {
        let mut s = Settings {
            master: 850,
            ..Settings::default()
        };
        s.step(None, true);
        assert_eq!(s.master, 900);
        s.step(None, false);
        s.step(None, false);
        assert_eq!(s.master, 700);
        s.set(None, 0);
        s.step(None, false);
        assert_eq!(s.master, 0);
        s.set(None, 1000);
        s.step(None, true);
        assert_eq!(s.master, 1000);
        s.set(None, 850);
        s.step(None, false);
        assert_eq!(s.master, 800);
    }
}
