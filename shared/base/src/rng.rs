//! Deterministic random numbers for rules: the same seed gives
//! the same sequence on every machine. SplitMix64; not for secrets.

/// Seeded by the zone; every draw advances the state, which is saved in
/// the zone snapshot (replay).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rng64(u64);

impl Rng64 {
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub const fn state(self) -> u64 {
        self.0
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `lo..=hi` (`lo` when `hi <= lo`).
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        let span = u64::from(hi - lo) + 1;
        lo + (self.next_u64() % span) as u32
    }

    /// True with `permille` / 1000 chance.
    pub fn chance(&mut self, permille: u32) -> bool {
        self.range(0, 999) < permille
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let (mut a, mut b) = (Rng64::new(7), Rng64::new(7));
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut r = Rng64::new(1);
        for _ in 0..1000 {
            assert!((3..=5).contains(&r.range(3, 5)));
        }
        assert!(!Rng64::new(3).chance(0));
        assert!(Rng64::new(3).chance(1000));
    }
}
