//! Hit, damage and defense formulas.
//!
//! Pure: sheets in, results
//! out; randomness comes from the caller's `Rng64`. Integers in thousandths,
//! never floating point.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
#![deny(clippy::float_arithmetic)]

use base::{Attribute, Rng64};
use defs::{Attributes, CombatFormulas};

/// What a fighter brings to one blow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombatSheet {
    pub level: u8,
    pub attributes: Attributes,
    /// Weapon damage range (unarmed from the formulas when no weapon).
    pub weapon_damage: (u32, u32),
    pub weapon_attribute: Attribute,
    /// Proficiency of the weapon in use, 0 to 100.
    pub weapon_prof: u8,
    /// Armor.
    pub defense: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitResult {
    Missed,
    Hit { damage: u32, crit: bool },
}

/// Chance to hit in thousandths: `scale × acc / (acc + dodge)`, clamped.
pub fn hit_chance(f: &CombatFormulas, at: &CombatSheet, df: &CombatSheet) -> u32 {
    let acc = u64::from(at.attributes.dex) + u64::from(at.weapon_prof) + 1;
    let dodge = u64::from(df.attributes.agi) + 1;
    let raw = u64::from(f.hit_scale_permille) * acc / (acc + dodge);
    (raw as u32).clamp(f.hit_min_permille, f.hit_max_permille)
}

pub fn crit_chance(f: &CombatFormulas, at: &CombatSheet) -> u32 {
    f.crit_base_permille
        .saturating_add(
            f.crit_per_dex_permille
                .saturating_mul(u32::from(at.attributes.dex)),
        )
        .min(f.crit_max_permille)
}

/// Damage multiplier for the level gap in PvE, in thousandths: each level
/// the target has above the attacker takes `step` off, and adds it the
/// other way, capped.
pub fn level_factor(f: &CombatFormulas, at_level: u8, df_level: u8) -> u32 {
    let gap = i64::from(df_level) - i64::from(at_level);
    let cap = i64::from(f.level_diff_cap_permille);
    let delta = (gap * i64::from(f.level_diff_step_permille)).clamp(-cap, cap);
    (1000 - delta).max(0) as u32
}

/// One blow. `mult` is the technique multiplier in thousandths (1000 for
/// the basic attack); `pvp` applies the PvP damage factor and skips the
/// level gap.
pub fn resolve_hit(
    f: &CombatFormulas,
    at: &CombatSheet,
    df: &CombatSheet,
    mult: u32,
    pvp: bool,
    rng: &mut Rng64,
) -> HitResult {
    if !rng.chance(hit_chance(f, at, df)) {
        return HitResult::Missed;
    }
    // (attribute × coef + weapon) × mult × (1 + prof/divisor) × variance.
    let weapon = u64::from(rng.range(at.weapon_damage.0, at.weapon_damage.1));
    let attr = u64::from(at.attributes.get(at.weapon_attribute)) * u64::from(f.attribute_coef_permille);
    let mut dmg = (attr + weapon * 1000) * u64::from(mult) / 1000; // thousandths of a point
    dmg = dmg * (u64::from(f.prof_damage_divisor) + u64::from(at.weapon_prof))
        / u64::from(f.prof_damage_divisor);
    dmg = dmg * u64::from(rng.range(f.variance_min_permille, f.variance_max_permille)) / 1000;
    let crit = rng.chance(crit_chance(f, at));
    if crit {
        dmg = dmg * u64::from(f.crit_damage_permille) / 1000;
    }
    dmg = if pvp {
        dmg * u64::from(f.pvp_damage_permille) / 1000
    } else {
        dmg * u64::from(level_factor(f, at.level, df.level)) / 1000
    };
    // Diminishing returns: × 100 / (100 + defense); never below 1.
    dmg = dmg * 100 / (100 + u64::from(df.defense));
    let damage = u32::try_from(dmg / 1000).unwrap_or(u32::MAX).max(1);
    HitResult::Hit { damage, crit }
}

/// Who a creature fights: damage dealt to it by each attacker, decaying.
/// Keys are entity ids (any `Ord` type the caller uses).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ThreatTable<K> {
    entries: Vec<(K, u64)>,
}

impl<K: Copy + Ord> ThreatTable<K> {
    pub fn add(&mut self, who: K, amount: u64) {
        match self.entries.binary_search_by(|(k, _)| k.cmp(&who)) {
            Ok(i) => {
                if let Some(e) = self.entries.get_mut(i) {
                    e.1 = e.1.saturating_add(amount);
                }
            }
            Err(i) => self.entries.insert(i, (who, amount)),
        }
    }

    pub fn remove(&mut self, who: K) {
        self.entries.retain(|(k, _)| *k != who);
    }

    /// Keeps `keep` thousandths of every entry; drops the ones at zero.
    pub fn decay(&mut self, keep: u32) {
        for e in &mut self.entries {
            e.1 = e.1 * u64::from(keep) / 1000;
        }
        self.entries.retain(|(_, v)| *v > 0);
    }

    /// Highest threat; ties go to the lowest key (deterministic).
    pub fn top(&self) -> Option<K> {
        let mut best: Option<(K, u64)> = None;
        for &(k, v) in &self.entries {
            if best.is_none_or(|(_, b)| v > b) {
                best = Some((k, v));
            }
        }
        best.map(|(k, _)| k)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Threat of one attacker (0 when absent).
    pub fn get(&self, who: K) -> u64 {
        self.entries
            .binary_search_by(|(k, _)| k.cmp(&who))
            .ok()
            .and_then(|i| self.entries.get(i))
            .map_or(0, |e| e.1)
    }

    /// Sum of every attacker's threat.
    pub fn total(&self) -> u64 {
        self.entries.iter().fold(0u64, |a, e| a.saturating_add(e.1))
    }

    /// Thousandths of the total that `who` holds (0 on an empty table).
    pub fn share_permille(&self, who: K) -> u32 {
        let total = self.total();
        if total == 0 {
            return 0;
        }
        u32::try_from(u128::from(self.get(who)) * 1000 / u128::from(total)).unwrap_or(1000)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = K> + '_ {
        self.entries.iter().map(|(k, _)| *k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(level: u8, a: u16) -> CombatSheet {
        CombatSheet {
            level,
            attributes: Attributes::all(a),
            weapon_damage: (4, 8),
            weapon_attribute: Attribute::Str,
            weapon_prof: 0,
            defense: 0,
        }
    }

    #[test]
    fn hit_chance_is_bounded_and_even_is_80() {
        let f = CombatFormulas::default();
        let (a, b) = (sheet(1, 10), sheet(1, 10));
        // acc 11, dodge 11: 1600 × 11 / 22 = 800.
        assert_eq!(hit_chance(&f, &a, &b), 800);
        assert_eq!(hit_chance(&f, &sheet(1, 0), &sheet(1, 1000)), 50);
        assert_eq!(hit_chance(&f, &sheet(1, 1000), &sheet(1, 0)), 950);
    }

    #[test]
    fn damage_is_positive_and_bounded() {
        let f = CombatFormulas::default();
        let mut rng = Rng64::new(42);
        let (a, mut d) = (sheet(5, 10), sheet(5, 10));
        let (mut hits, mut total) = (0u64, 0u64);
        for _ in 0..10_000 {
            if let HitResult::Hit { damage, .. } = resolve_hit(&f, &a, &d, 1000, false, &mut rng) {
                // (10×0.5 + 4..8) × 0.9..1.1 × up to 1.5 crit.
                assert!((1..=22).contains(&damage), "{damage}");
                hits += 1;
                total += u64::from(damage);
            }
        }
        assert!((7_500..8_500).contains(&hits), "{hits}");
        let avg_open = total / hits;
        d.defense = 100;
        let mut total_armored = 0u64;
        let mut n = 0u64;
        for _ in 0..10_000 {
            if let HitResult::Hit { damage, .. } = resolve_hit(&f, &a, &d, 1000, false, &mut rng) {
                total_armored += u64::from(damage);
                n += 1;
            }
        }
        assert!(total_armored / n < avg_open, "armor halves damage");
    }

    #[test]
    fn level_gap_and_threat() {
        let f = CombatFormulas::default();
        assert_eq!(level_factor(&f, 1, 1), 1000);
        assert_eq!(level_factor(&f, 1, 3), 940);
        assert_eq!(level_factor(&f, 1, 50), 700);
        assert_eq!(level_factor(&f, 50, 1), 1300);
        let mut t = ThreatTable::default();
        t.add(3u32, 10);
        t.add(1, 10);
        t.add(2, 5);
        assert_eq!(t.top(), Some(1));
        t.add(2, 20);
        assert_eq!(t.top(), Some(2));
        assert_eq!(t.get(2), 25);
        assert_eq!(t.total(), 45);
        assert_eq!(t.share_permille(2), 555);
        assert_eq!(t.share_permille(9), 0);
        t.decay(0);
        assert!(t.is_empty());
        assert_eq!(t.share_permille(1), 0);
    }
}
