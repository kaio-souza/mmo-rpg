//! Balance numbers (phase 4): `content/progression/formulas.ron`
//! and `content/combat/formulas.ron`. Both files are optional; every field
//! has the value of the design docs as default. Ratios are integers in
//! thousandths (`_permille`), the same fixed point as `base::Fixed`: no
//! floating point in rules.

use base::Attribute;
use serde::{Deserialize, Serialize};

/// The six attributes (STR, VIT, AGI, DEX, INT, SPI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, default)]
pub struct Attributes {
    pub str: u16,
    pub vit: u16,
    pub agi: u16,
    pub dex: u16,
    pub int: u16,
    pub spi: u16,
}

impl Attributes {
    pub const fn all(v: u16) -> Attributes {
        Attributes {
            str: v,
            vit: v,
            agi: v,
            dex: v,
            int: v,
            spi: v,
        }
    }

    pub fn get(&self, a: Attribute) -> u16 {
        match a {
            Attribute::Str => self.str,
            Attribute::Vit => self.vit,
            Attribute::Agi => self.agi,
            Attribute::Dex => self.dex,
            Attribute::Int => self.int,
            Attribute::Spi => self.spi,
        }
    }

    pub fn get_mut(&mut self, a: Attribute) -> &mut u16 {
        match a {
            Attribute::Str => &mut self.str,
            Attribute::Vit => &mut self.vit,
            Attribute::Agi => &mut self.agi,
            Attribute::Dex => &mut self.dex,
            Attribute::Int => &mut self.int,
            Attribute::Spi => &mut self.spi,
        }
    }

    pub fn total(&self) -> u32 {
        Attribute::ALL.iter().map(|a| u32::from(self.get(*a))).sum()
    }

    /// Sum field by field, saturating.
    pub fn plus(&self, o: &Attributes) -> Attributes {
        let mut r = *self;
        for a in Attribute::ALL {
            let v = r.get_mut(a);
            *v = v.saturating_add(o.get(a));
        }
        r
    }
}

/// `content/progression/formulas.ron`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, default)]
pub struct ProgressionFormulas {
    pub max_level: u8,
    /// XP from level `n` to `n + 1`: `base × n^(num/den)`.
    pub level_xp_base: u64,
    pub level_xp_exponent: (u32, u32),
    /// Every attribute of a new character.
    pub start_attribute: u16,
    /// Free points per level above the highest level reached, with a class.
    pub free_points: u16,
    /// The same without a class.
    pub free_points_without_class: u16,
    /// XP for proficiency point `p + 1`: `base × (growth/1000)^p`.
    pub prof_xp_base: u32,
    pub prof_xp_growth_permille: u32,
    pub prof_max: u8,
    /// Effective uses for technique level `t + 1`: `base × 2^t`.
    pub technique_uses_base: u32,
    pub technique_max: u8,
    pub hp_base: u32,
    pub hp_per_vit: u32,
    pub hp_per_level: u32,
    pub vigor_base: u32,
    pub vigor_per_vit: u32,
    pub vigor_per_agi: u32,
    pub mp_base: u32,
    pub mp_per_int: u32,
    pub mp_per_level: u32,
    /// Regeneration runs every this many ticks.
    pub regen_interval_ticks: u32,
    /// Share of the maximum regained per interval out of combat.
    pub hp_regen_permille: u32,
    pub vigor_regen_permille: u32,
    /// MP regained per interval, always (in or out of combat): this share
    /// of the maximum plus `mp_regen_per_spi_permille` for each point of SPI.
    pub mp_regen_permille: u32,
    pub mp_regen_per_spi_permille: u32,
    /// Ticks without hitting or being hit to count as out of combat.
    pub out_of_combat_ticks: u32,
    /// Fallen, waiting for resurrection (: 60 s).
    pub fallen_ticks: u32,
    /// Up to this level a death costs no level and no lock.
    pub newbie_level: u8,
    /// Seconds a dead character cannot enter (: 6 h).
    pub death_lock_secs: u64,
    /// Phase 10: length of a day of the world in real seconds;
    /// one clock for every zone, from the gateway's time.
    pub day_secs: u32,
    /// Companions a character may keep (tamed monsters); one follows at a time.
    pub max_companions: u8,
    /// Seconds a companion that fell rests before it can be called again.
    pub companion_rest_secs: u32,
    /// Quests of a rank a character completes to reach the next rank.
    pub quests_per_rank: u8,
    /// Regeneration while resting (sitting, out of combat), in thousandths
    /// of the normal one.
    pub rest_regen_permille: u32,
    /// Night for the rules (dialogues, spawns): from this share of the
    /// world day to that one, in thousandths (midnight is 0).
    pub night_from_permille: u16,
    pub night_until_permille: u16,
}

impl ProgressionFormulas {
    /// Whether the world is in its night at this wall-clock time.
    pub fn is_night(&self, now_unix: u64) -> bool {
        let day = u64::from(self.day_secs.max(1));
        let at = u16::try_from(u64::from(self.time_of_day(now_unix)).saturating_mul(1000) / day).unwrap_or(0);
        let (from, until) = (self.night_from_permille, self.night_until_permille);
        if from <= until {
            (from..until).contains(&at)
        } else {
            at >= from || at < until
        }
    }

    /// The world hour (24 per world day) at this wall-clock time.
    pub fn world_hour(&self, now_unix: u64) -> u64 {
        let day_secs = self.day_secs.max(1);
        base::world_hour(
            now_unix / u64::from(day_secs),
            self.time_of_day(now_unix),
            day_secs,
        )
    }

    /// Seconds since the world's midnight at this wall-clock time.
    pub fn time_of_day(&self, now_unix: u64) -> u32 {
        u32::try_from(now_unix % u64::from(self.day_secs.max(1))).unwrap_or(0)
    }
}

impl Default for ProgressionFormulas {
    fn default() -> Self {
        ProgressionFormulas {
            max_level: 100,
            level_xp_base: 100,
            level_xp_exponent: (11, 5),
            start_attribute: 5,
            free_points: 3,
            free_points_without_class: 5,
            prof_xp_base: 20,
            prof_xp_growth_permille: 1070,
            prof_max: 100,
            technique_uses_base: 10,
            technique_max: 10,
            hp_base: 30,
            hp_per_vit: 6,
            hp_per_level: 4,
            vigor_base: 40,
            vigor_per_vit: 2,
            vigor_per_agi: 2,
            mp_base: 20,
            mp_per_int: 3,
            mp_per_level: 1,
            regen_interval_ticks: 20,
            hp_regen_permille: 30,
            vigor_regen_permille: 80,
            mp_regen_permille: 30,
            mp_regen_per_spi_permille: 6,
            out_of_combat_ticks: 100,
            fallen_ticks: 1200,
            newbie_level: 10,
            death_lock_secs: 6 * 3600,
            day_secs: 2 * 3600,
            max_companions: 3,
            companion_rest_secs: 60,
            quests_per_rank: 5,
            rest_regen_permille: 3000,
            night_from_permille: 800,
            night_until_permille: 250,
        }
    }
}

/// `content/combat/formulas.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, default)]
pub struct CombatFormulas {
    pub hit_min_permille: u32,
    pub hit_max_permille: u32,
    /// Hit chance before the clamp: `scale × accuracy / (accuracy + dodge)`.
    pub hit_scale_permille: u32,
    pub crit_base_permille: u32,
    pub crit_per_dex_permille: u32,
    pub crit_max_permille: u32,
    pub crit_damage_permille: u32,
    pub variance_min_permille: u32,
    pub variance_max_permille: u32,
    /// Damage share of the weapon attribute.
    pub attribute_coef_permille: u32,
    /// Damage `× (1 + prof / divisor)`.
    pub prof_damage_divisor: u32,
    /// Per level of difference (PvE), capped.
    pub level_diff_step_permille: u32,
    pub level_diff_cap_permille: u32,
    pub pvp_damage_permille: u32,
    pub unarmed_damage: (u32, u32),
    pub unarmed_attack_ticks: u32,
    pub global_cooldown_ticks: u32,
    /// A heal or a buff grows by this many thousandths per point
    /// of the technique's proficiency in use (faith adds to light).
    pub support_per_point_permille: u32,
}

impl Default for CombatFormulas {
    fn default() -> Self {
        CombatFormulas {
            hit_min_permille: 50,
            hit_max_permille: 950,
            hit_scale_permille: 1600,
            crit_base_permille: 50,
            crit_per_dex_permille: 1,
            crit_max_permille: 500,
            crit_damage_permille: 1500,
            variance_min_permille: 900,
            variance_max_permille: 1100,
            attribute_coef_permille: 500,
            prof_damage_divisor: 200,
            level_diff_step_permille: 30,
            level_diff_cap_permille: 300,
            pvp_damage_permille: 600,
            unarmed_damage: (1, 3),
            unarmed_attack_ticks: 30,
            global_cooldown_ticks: 10,
            support_per_point_permille: 2,
        }
    }
}

/// `content/economy/formulas.ron` (phase 7).
/// Prices are shares of the item's `value`; the shop of each city varies
/// them with its own `price_permille`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, default)]
pub struct EconomyFormulas {
    /// What the player pays an NPC, in thousandths of the value (100%).
    pub sell_permille: u32,
    /// What an NPC pays the player (30%): the difference is a coin sink.
    pub buy_permille: u32,
    /// Full repair cost, in thousandths of the value missing.
    pub repair_permille: u32,
    /// An NPC repair lowers the maximum durability by this much of it
    pub repair_max_loss_permille: u32,
    /// Alpha: an NPC appraisal costs this much of the piece's value
    /// (always reveals).
    pub appraise_permille: u32,
    /// Durability a weapon loses per blow landed, and armor per blow taken.
    pub durability_per_hit: u32,
    /// Durability lost on death, in thousandths of the maximum.
    pub death_durability_permille: u32,
    /// The System warns once below each of these shares of the maximum.
    pub warn_durability_permille: (u32, u32),
    /// Level XP of a craft without a production class, in thousandths of
    /// the recipe's XP (×0.02).
    pub craft_xp_permille: u32,
    /// Chance of one rarity step up when crafting: base, per proficiency
    /// point and per DEX, in thousandths, capped. Every further step
    /// halves the chance.
    pub craft_quality_permille: u32,
    pub craft_quality_per_point_permille: u32,
    pub craft_quality_per_dex_permille: u32,
    pub craft_quality_max_permille: u32,
    /// Ticks a node takes to come back when it says nothing.
    pub gather_cooldown_ticks: u32,
    /// By `Rarity` (Common to Mythic), in thousandths added on top: the
    /// weapon's damage or the armor's defense, and the durability of the
    /// piece.
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ByRarity"))]
    pub rarity_power_permille: [u32; 6],
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ByRarity"))]
    pub rarity_durability_permille: [u32; 6],
    /// By `Rarity`, the value of a piece in thousandths of a common one's
    /// 0: no NPC buys it.
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ByRarity"))]
    pub rarity_value_permille: [u32; 6],
    /// By `Rarity`, the chance of a piece rolling one of its item's powers.
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ByRarity"))]
    pub power_chance_permille: [u32; 6],
    /// By `Rarity`, the chance of a piece rolling its item's hidden
    /// attributes at all; each then rolls `min.=max`.
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ByRarity"))]
    pub hidden_chance_permille: [u32; 6],
    /// Mastery of a production class: extra durability of what it crafts,
    /// in hundredths of a percent, `points × DEX × tier / divisor`, capped
    /// (tier: the class's tier; 0 without a production class).
    pub mastery_divisor: u32,
    pub mastery_max: u32,
    /// The rarest piece of equipment an NPC still buys; above it, only
    /// players (: Epic and up go to the market).
    pub npc_buy_max: base::Rarity,
}

impl EconomyFormulas {
    /// Thousandths of `r` in a by-rarity table.
    pub fn by_rarity(table: &[u32; 6], r: base::Rarity) -> u32 {
        table.get(r as usize).copied().unwrap_or(0)
    }

    /// Mastery of a crafter, in hundredths of a percent (0 to `mastery_max`).
    pub fn mastery(&self, points: u8, dex: u16, tier: u8) -> u16 {
        let tier = u64::from(tier);
        let raw = u64::from(points) * u64::from(dex) * tier / u64::from(self.mastery_divisor.max(1));
        raw.min(u64::from(self.mastery_max)) as u16
    }
}

impl Default for EconomyFormulas {
    fn default() -> Self {
        EconomyFormulas {
            sell_permille: 1000,
            buy_permille: 300,
            repair_permille: 500,
            repair_max_loss_permille: 20,
            appraise_permille: 100,
            durability_per_hit: 1,
            death_durability_permille: 100,
            warn_durability_permille: (200, 50),
            craft_xp_permille: 20,
            craft_quality_permille: 20,
            craft_quality_per_point_permille: 2,
            craft_quality_per_dex_permille: 1,
            craft_quality_max_permille: 250,
            gather_cooldown_ticks: 600,
            rarity_power_permille: [0, 100, 200, 300, 450, 600],
            rarity_durability_permille: [0, 250, 500, 750, 1000, 1500],
            rarity_value_permille: [1000, 2000, 5000, 15000, 50000, 0],
            power_chance_permille: [0, 0, 0, 250, 500, 1000],
            hidden_chance_permille: [100, 300, 1000, 1000, 1000, 1000],
            mastery_divisor: 5,
            mastery_max: 1000,
            npc_buy_max: base::Rarity::Rare,
        }
    }
}

/// Both formula files, checked, with the level table computed once.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Formulas {
    pub progression: ProgressionFormulas,
    pub combat: CombatFormulas,
    pub economy: EconomyFormulas,
    /// `level_xp[n - 1]`: XP from level `n` to `n + 1`.
    level_xp: Vec<u64>,
    /// `prof_xp[p]`: XP for proficiency point `p + 1`.
    prof_xp: Vec<u64>,
}

impl Formulas {
    /// XP to go from `level` to `level + 1` (`u64::MAX` at or past the cap).
    pub fn level_xp(&self, level: u8) -> u64 {
        usize::from(level)
            .checked_sub(1)
            .and_then(|i| self.level_xp.get(i))
            .copied()
            .unwrap_or(u64::MAX)
    }

    /// XP for proficiency point `points + 1` (`u64::MAX` at the cap).
    pub fn prof_xp(&self, points: u8) -> u64 {
        self.prof_xp.get(usize::from(points)).copied().unwrap_or(u64::MAX)
    }

    /// Effective uses to raise a technique from `level` to `level + 1`.
    pub fn technique_uses(&self, level: u8) -> u64 {
        if level >= self.progression.technique_max {
            return u64::MAX;
        }
        u64::from(self.progression.technique_uses_base)
            .checked_shl(u32::from(level).saturating_sub(1))
            .unwrap_or(u64::MAX)
    }

    pub fn max_hp(&self, a: &Attributes, level: u8) -> u32 {
        let p = &self.progression;
        p.hp_base
            .saturating_add(p.hp_per_vit.saturating_mul(u32::from(a.vit)))
            .saturating_add(p.hp_per_level.saturating_mul(u32::from(level)))
    }

    pub fn max_mp(&self, a: &Attributes, level: u8) -> u32 {
        let p = &self.progression;
        p.mp_base
            .saturating_add(p.mp_per_int.saturating_mul(u32::from(a.int)))
            .saturating_add(p.mp_per_level.saturating_mul(u32::from(level)))
    }

    /// MP regained per regeneration interval (at least 1).
    pub fn mp_regen(&self, a: &Attributes, max_mp: u32) -> u32 {
        let p = &self.progression;
        let permille =
            u64::from(p.mp_regen_permille) + u64::from(p.mp_regen_per_spi_permille) * u64::from(a.spi);
        ((u64::from(max_mp) * permille / 1000).max(1)).min(u64::from(u32::MAX)) as u32
    }

    pub fn max_vigor(&self, a: &Attributes) -> u32 {
        let p = &self.progression;
        p.vigor_base
            .saturating_add(p.vigor_per_vit.saturating_mul(u32::from(a.vit)))
            .saturating_add(p.vigor_per_agi.saturating_mul(u32::from(a.agi)))
    }
}

/// Largest `r` with `r^q <= v`.
fn int_root(v: u128, q: u32) -> u128 {
    let (mut lo, mut hi) = (0u128, 1u128 << (128 / q.max(1)).min(127));
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        match mid.checked_pow(q) {
            Some(p) if p <= v => lo = mid,
            _ => hi = mid - 1,
        }
    }
    lo
}

pub(crate) fn parse_formulas(
    progression: Option<&str>,
    combat: Option<&str>,
    economy: Option<&str>,
) -> Result<Formulas, Vec<String>> {
    let mut errors = Vec::new();
    let progression: ProgressionFormulas = match progression.map(ron::from_str) {
        None => ProgressionFormulas::default(),
        Some(Ok(p)) => p,
        Some(Err(e)) => return Err(vec![format!("progression/formulas.ron: {e}")]),
    };
    let combat: CombatFormulas = match combat.map(ron::from_str) {
        None => CombatFormulas::default(),
        Some(Ok(c)) => c,
        Some(Err(e)) => return Err(vec![format!("combat/formulas.ron: {e}")]),
    };
    let economy: EconomyFormulas = match economy.map(ron::from_str) {
        None => EconomyFormulas::default(),
        Some(Ok(c)) => c,
        Some(Err(e)) => return Err(vec![format!("economy/formulas.ron: {e}")]),
    };
    let (num, den) = progression.level_xp_exponent;
    if progression.max_level < 2 || den == 0 || num == 0 || num > 4 * den || den > 8 {
        errors.push(
            "progression/formulas.ron: max_level >= 2, exponent num/den with 0 < num <= 4 den, den <= 8"
                .into(),
        );
        return Err(errors);
    }
    // base × n^(num/den) = base × root_den(n^num × S^den) / S, S = 100.
    let mut level_xp = Vec::new();
    for n in 1..u128::from(progression.max_level) {
        let v = n
            .checked_pow(num)
            .and_then(|x| x.checked_mul(100u128.pow(den)))
            .map(|x| int_root(x, den) * u128::from(progression.level_xp_base) / 100)
            .and_then(|x| u64::try_from(x).ok());
        match v {
            Some(x) => level_xp.push(x.max(1)),
            None => {
                errors.push(format!("progression/formulas.ron: XP of level {n} overflows"));
                break;
            }
        }
    }
    let mut prof_xp = Vec::new();
    let mut x = u128::from(progression.prof_xp_base).max(1) * 1000;
    for _ in 0..progression.prof_max {
        prof_xp.push(u64::try_from(x / 1000).unwrap_or(u64::MAX).max(1));
        x = (x * u128::from(progression.prof_xp_growth_permille) / 1000).min(u128::from(u64::MAX) * 1000);
    }
    let c = &combat;
    if c.hit_min_permille > c.hit_max_permille
        || c.hit_max_permille > 1000
        || c.variance_min_permille > c.variance_max_permille
        || c.unarmed_damage.0 > c.unarmed_damage.1
        || c.unarmed_attack_ticks == 0
        || c.prof_damage_divisor == 0
    {
        errors.push("combat/formulas.ron: min above max, hit above 1000, or a zero divisor/interval".into());
    }
    if progression.regen_interval_ticks == 0 || progression.prof_max > 100 {
        errors.push("progression/formulas.ron: regen_interval_ticks >= 1, prof_max <= 100".into());
    }
    let e = &economy;
    if e.buy_permille > e.sell_permille || e.sell_permille == 0 {
        errors.push("economy/formulas.ron: 0 < buy_permille <= sell_permille (an NPC never pays more than it charges)".into());
    }
    if e.repair_max_loss_permille > 1000
        || e.death_durability_permille > 1000
        || e.warn_durability_permille.0 > 1000
        || e.warn_durability_permille.1 > e.warn_durability_permille.0
    {
        errors.push(
            "economy/formulas.ron: shares of the maximum are at most 1000, with the first warning above the second"
                .into(),
        );
    }
    if e.power_chance_permille
        .iter()
        .chain(&e.hidden_chance_permille)
        .any(|c| *c > 1000)
        || e.mastery_divisor == 0
        || e.mastery_max > 10_000
    {
        errors.push(
            "economy/formulas.ron: power_chance_permille and hidden_chance_permille <= 1000, mastery_divisor >= 1, mastery_max <= 10000"
                .into(),
        );
    }
    if e.craft_quality_max_permille > 1000 || e.gather_cooldown_ticks == 0 {
        errors.push(
            "economy/formulas.ron: craft_quality_max_permille <= 1000, gather_cooldown_ticks >= 1".into(),
        );
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(Formulas {
        progression,
        combat,
        economy,
        level_xp,
        prof_xp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_design() {
        let f = parse_formulas(None, None, None).unwrap();
        // 100 × n^2.2: level 1 → 100, level 10 ≈ 15 849.
        assert_eq!(f.level_xp(1), 100);
        assert!((15_800..15_900).contains(&f.level_xp(10)), "{}", f.level_xp(10));
        assert_eq!(f.level_xp(100), u64::MAX);
        // 20 × 1.07^p: p 10 ≈ 39.
        assert_eq!(f.prof_xp(0), 20);
        assert!((38..=40).contains(&f.prof_xp(10)));
        assert_eq!(f.technique_uses(1), 10);
        assert_eq!(f.technique_uses(3), 40);
        assert_eq!(int_root(1_000_000, 3), 100);
    }

    #[test]
    fn bad_files() {
        assert!(parse_formulas(Some("(max_level: 1)"), None, None).is_err());
        assert!(parse_formulas(None, Some("(hit_max_permille: 2000)"), None).is_err());
        assert!(parse_formulas(Some("(nope: 1)"), None, None).is_err());
        // An NPC that pays more than it charges would print money.
        assert!(parse_formulas(None, None, Some("(buy_permille: 2000)")).is_err());
        let f = parse_formulas(None, None, Some("(buy_permille: 400)")).unwrap();
        assert_eq!(f.economy.buy_permille, 400);
    }
}
