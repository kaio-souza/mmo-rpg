//! Simulation time. Shared crates never read the clock; time arrives as `Tick`.

pub const TICKS_PER_SECOND: u64 = 20;
/// Ticks one walked tile takes: 4 at 20 Hz is 5 tiles per second.
pub const STEP_TICKS: u64 = 4;
/// Ticks a step along the screen's horizontal (NE, SW) takes: it crosses
/// twice the screen width of a straight step, so it takes longer and the
/// speed seen on screen stays even.
pub const WIDE_STEP_TICKS: u64 = 7;

/// The world hour (24 per world day) at `secs_of_day` of world day `day`.
pub fn world_hour(day: u64, secs_of_day: u32, day_secs: u32) -> u64 {
    let hour = u64::from(secs_of_day).saturating_mul(24) / u64::from(day_secs.max(1));
    day.saturating_mul(24).saturating_add(hour.min(23))
}

/// A roll 0..1000 for `zone` in world hour `hour`, the same on the server
/// and every client (the weather of the hour, picture only).
pub fn hour_roll(zone: &str, hour: u64) -> u16 {
    let mut h = crate::Fnv64::new();
    h.write_str(zone);
    h.write(&hour.to_le_bytes());
    u16::try_from(h.finish() % 1000).unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Tick(pub u64);

impl Tick {
    pub const ZERO: Tick = Tick(0);

    pub const fn next(self) -> Tick {
        Tick(self.0.saturating_add(1))
    }

    pub const fn after(self, ticks: u64) -> Tick {
        Tick(self.0.saturating_add(ticks))
    }

    /// Ticks elapsed since `earlier` (zero if `earlier` is later).
    pub const fn since(self, earlier: Tick) -> u64 {
        self.0.saturating_sub(earlier.0)
    }

    pub const fn from_secs(secs: u64) -> u64 {
        secs.saturating_mul(TICKS_PER_SECOND)
    }

    /// Ticks covering `millis`, rounded up (a 1 ms cooldown still lasts a tick).
    pub const fn from_millis(millis: u64) -> u64 {
        millis.saturating_mul(TICKS_PER_SECOND).div_ceil(1000)
    }

    pub const fn to_millis(ticks: u64) -> u64 {
        ticks.saturating_mul(1000 / TICKS_PER_SECOND)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(Tick::from_secs(60), 1200);
        assert_eq!(Tick::from_millis(50), 1);
        assert_eq!(Tick::from_millis(51), 2);
        assert_eq!(Tick::from_millis(0), 0);
        assert_eq!(Tick::to_millis(20), 1000);
        assert_eq!(Tick(10).since(Tick(4)), 6);
        assert_eq!(Tick(4).since(Tick(10)), 0);
        assert_eq!(Tick(u64::MAX).next(), Tick(u64::MAX));
    }
}
