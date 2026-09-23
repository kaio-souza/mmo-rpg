//! FNV-1a 64: stable across machines and Rust versions (unlike `DefaultHasher`).

/// Stable 64-bit hash for content hashes, replay and state checks.
#[derive(Clone, Debug)]
pub struct Fnv64(u64);

impl Default for Fnv64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv64 {
    pub const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    /// Writes the length first, so `("ab", "c")` and `("a", "bc")` differ.
    pub fn write_str(&mut self, s: &str) {
        self.write(&(s.len() as u64).to_le_bytes());
        self.write(s.as_bytes());
    }

    pub fn write_u64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }

    pub fn write_i32(&mut self, v: i32) {
        self.write(&v.to_le_bytes());
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

/// Stable 64-bit hash for the per-tick state check of the replay.
/// It takes 8 bytes per step where [`Fnv64`] takes one: the zone hashes
/// about 1 KB per player every tick. Same value on every machine
/// (little-endian words); not the content hash, which stays [`Fnv64`].
#[derive(Clone, Debug)]
pub struct StateHash(u64);

impl Default for StateHash {
    fn default() -> Self {
        Self::new()
    }
}

impl StateHash {
    const K: u64 = 0x9e37_79b9_7f4a_7c15;

    pub const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn mix(&mut self, w: u64) {
        let x = (self.0 ^ w).wrapping_mul(Self::K);
        self.0 = x ^ (x >> 29);
    }

    /// The length goes in too, so `([1], [2])` and `([1, 2])` differ.
    pub fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            let mut w = [0u8; 8];
            w.copy_from_slice(c);
            self.mix(u64::from_le_bytes(w));
        }
        let rest = chunks.remainder();
        let mut w = [0u8; 8];
        if let Some(dst) = w.get_mut(..rest.len()) {
            dst.copy_from_slice(rest);
        }
        self.mix(u64::from_le_bytes(w) ^ ((bytes.len() as u64) << 56));
    }

    pub fn write_str(&mut self, s: &str) {
        self.write(s.as_bytes());
    }

    pub fn write_u64(&mut self, v: u64) {
        self.mix(v);
    }

    pub fn write_i32(&mut self, v: i32) {
        self.mix(u64::from(u32::from_le_bytes(v.to_le_bytes())));
    }

    pub fn finish(&self) -> u64 {
        let x = (self.0 ^ (self.0 >> 31)).wrapping_mul(Self::K);
        x ^ (x >> 32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_value() {
        let mut h = Fnv64::new();
        h.write(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn state_hash_is_stable_and_splits_writes() {
        let mut h = StateHash::new();
        h.write(b"abcdefghij");
        h.write_u64(7);
        h.write_i32(-1);
        h.write_str("x");
        assert_eq!(h.finish(), STATE_KNOWN);
        let (mut a, mut b) = (StateHash::new(), StateHash::new());
        a.write(&[1]);
        a.write(&[2]);
        b.write(&[1, 2]);
        assert_ne!(a.finish(), b.finish());
        let (mut a, mut b) = (StateHash::new(), StateHash::new());
        a.write(&[0]);
        b.write(&[0, 0]);
        assert_ne!(a.finish(), b.finish());
    }

    const STATE_KNOWN: u64 = 1_624_080_670_560_642_222;
}
