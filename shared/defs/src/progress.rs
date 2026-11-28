//! Combat and progression content (phase 4): monsters, classes,
//! proficiencies, techniques, titles and the declared counter families.
//! Formats. Names and
//! descriptions are always `<kind>.<id>.name` / `<kind>.<id>.desc`.

use crate::formulas::Attributes;
use crate::npc::valid_id;
use base::{Attribute, ClassId, ItemId, LootTableId, ProficiencyId, Rarity, TechniqueId, TextId, TitleId};
use serde::{Deserialize, Serialize};

/// Counter families the engine knows how to count. Content
/// declares which are on in `counters.ron`; only those are counted.
pub const COUNTER_FAMILIES: [&str; 4] = ["kills", "deaths", "quests", "clears"];

/// What the key of each family names: `kills` counts per monster id and
/// `quests` per quest id. A family not listed has no key (only its total,
/// key `""`). The editor offers these ids instead of free typing.
pub const COUNTER_KEYS: [(&str, &str); 3] =
    [("kills", "monster"), ("quests", "quest"), ("clears", "dungeon")];

/// The content kind the key of `family` names, if it has keys.
pub fn counter_key_kind(family: &str) -> Option<&'static str> {
    COUNTER_KEYS.iter().find(|(f, _)| *f == family).map(|(_, k)| *k)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LootDef {
    pub item: ItemId,
    /// Chance in thousandths per kill.
    pub chance: u16,
    pub qty: (u16, u16),
}

/// A timed effect a blow leaves on its target. New kinds are appended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum StatusKind {
    /// Loses `power` HP every second.
    Poison,
    /// Cannot attack, use techniques or walk.
    Stun,
    /// Blows come `power` thousandths slower (500: half as often again).
    Slow,
}

/// A status a technique or a monster's blow leaves on whoever it hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub kind: StatusKind,
    /// How long it lasts (20 ticks = 1 s).
    pub duration_ticks: u32,
    /// Poison: HP per second. Slow: thousandths added to the wait between
    /// blows. Stun: unused.
    #[serde(default)]
    pub power: u32,
}

/// Longest status (5 minutes).
pub const MAX_STATUS_TICKS: u32 = 6000;

fn check_status(s: &Status) -> Result<(), String> {
    if s.duration_ticks == 0 || s.duration_ticks > MAX_STATUS_TICKS {
        return Err(format!("status duration_ticks must be 1 to {MAX_STATUS_TICKS}"));
    }
    if matches!(s.kind, StatusKind::Poison | StatusKind::Slow) && s.power == 0 {
        return Err("a Poison or Slow status needs power >= 1".into());
    }
    Ok(())
}

/// Largest area of a technique or a phase, in tiles around the target.
pub const MAX_AREA: u16 = 8;

/// A stage of a monster's fight (a boss), from when its HP drops to
/// `below_hp_permille` thousandths of the maximum. Later phases have lower
/// thresholds; the deepest reached holds until the monster resets.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MonsterPhase {
    /// HP left, in thousandths of the maximum (500: half), that starts it.
    pub below_hp_permille: u16,
    /// Damage of its blows, in thousandths (1000: as usual).
    #[serde(default = "thousand")]
    pub damage_permille: u32,
    /// Ticks between blows in this phase (`None`: the monster's own).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_ticks: Option<u32>,
    /// Its blows also hit every player within this many tiles of the
    /// target (0: only the target).
    #[serde(default)]
    pub area: u16,
    /// What its blows leave on whoever they hit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

fn thousand() -> u32 {
    1000
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonsterDef {
    pub id: String,
    pub name: TextId,
    pub level: u8,
    pub hp: u32,
    pub attributes: Attributes,
    pub damage: (u32, u32),
    pub defense: u32,
    pub weapon_prof: u8,
    pub attack_ticks: u32,
    pub aggro_radius: u16,
    /// Gives up and walks home past this distance from its spawn.
    pub leash: u16,
    /// Alpha: tiles its blow reaches. Above 1 the monster keeps away from
    /// a target that comes next to it (a caster).
    pub range: u16,
    pub respawn_ticks: u32,
    pub xp: u64,
    pub sprite: String,
    pub icon: String,
    pub coins: (u32, u32),
    pub loot: Vec<LootDef>,
    /// Shared table rolled on top of `loot` (phase 7).
    pub loot_table: Option<LootTableId>,
    /// Stages of the fight, highest threshold first.
    pub phases: Vec<MonsterPhase>,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct LootFile {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
    pub item: String,
    pub chance: u16,
    #[serde(default = "one_one")]
    pub qty: (u16, u16),
}

fn one_range() -> u16 {
    1
}

fn one_one() -> (u16, u16) {
    (1, 1)
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct MonsterFile {
    pub level: u8,
    pub hp: u32,
    #[serde(default)]
    pub attributes: Attributes,
    pub damage: (u32, u32),
    #[serde(default)]
    pub defense: u32,
    #[serde(default)]
    pub weapon_prof: u8,
    #[serde(default = "thirty")]
    pub attack_ticks: u32,
    #[serde(default = "five")]
    pub aggro_radius: u16,
    #[serde(default = "twelve")]
    pub leash: u16,
    #[serde(default = "one_range")]
    pub range: u16,
    #[serde(default = "respawn")]
    pub respawn_ticks: u32,
    pub xp: u64,
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::SpriteRef"))]
    pub sprite: String,
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::IconRef"))]
    pub icon: String,
    #[serde(default)]
    pub coins: (u32, u32),
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loot: Vec<LootFile>,
    /// Empty: only the monster's own `loot` lines.
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::LootTableRef"))]
    pub loot_table: String,
    /// Stages of the fight (a boss): from a share of HP down, harder blows,
    /// another pace, an area and a status. Highest threshold first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<MonsterPhase>,
}

fn thirty() -> u32 {
    30
}
fn five() -> u16 {
    5
}
fn twelve() -> u16 {
    12
}
fn respawn() -> u32 {
    600
}

pub(crate) fn parse_monster(id: &str, text: &str) -> Result<MonsterFile, String> {
    if !valid_id(id) {
        return Err(format!("monster id `{id}` must be [a-z0-9_]"));
    }
    let f: MonsterFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if f.level == 0 || f.hp == 0 || f.damage.0 > f.damage.1 || f.attack_ticks == 0 || f.coins.0 > f.coins.1 {
        return Err("level, hp and attack_ticks must be >= 1; damage and coins min <= max".into());
    }
    if f.leash < f.aggro_radius || f.respawn_ticks == 0 {
        return Err("leash must be >= aggro_radius; respawn_ticks >= 1".into());
    }
    check_loot(&f.loot)?;
    let mut last = 1001;
    for p in &f.phases {
        if p.below_hp_permille == 0 || p.below_hp_permille >= last {
            return Err("phases: below_hp_permille 1 to 1000, each lower than the one before".into());
        }
        last = p.below_hp_permille;
        if p.area > MAX_AREA || p.attack_ticks == Some(0) {
            return Err(format!("phases: area at most {MAX_AREA}, attack_ticks >= 1"));
        }
        if let Some(s) = &p.status {
            check_status(s)?;
        }
    }
    Ok(f)
}

pub fn monster_name_key(id: &str) -> String {
    format!("monster.{id}.name")
}

/// One requirement to earn a class (all of a class's list must hold).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassReq {
    Attribute(Attribute, u16),
    Proficiency(ProficiencyId, u8),
    /// At least one of these.
    Any(Vec<ClassReq>),
    /// Owning this title: a deed, so a class can be earned by deeds
    /// (a rare class whose door is a hidden title).
    Title(TitleId),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub(crate) enum ClassReqFile {
    Attribute(Attribute, u16),
    Proficiency(
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ProficiencyRef"))] String,
        u8,
    ),
    Any(Vec<ClassReqFile>),
    Title(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::TitleRef"))] String),
}

pub(crate) fn resolve_req(
    r: ClassReqFile,
    profs: &std::collections::BTreeMap<String, ProficiencyId>,
    titles: &std::collections::BTreeMap<String, TitleId>,
) -> Result<ClassReq, String> {
    Ok(match r {
        ClassReqFile::Attribute(a, v) => ClassReq::Attribute(a, v),
        ClassReqFile::Proficiency(p, v) => match profs.get(&p) {
            Some(&id) => ClassReq::Proficiency(id, v),
            None => return Err(format!("proficiency `{p}` not found")),
        },
        ClassReqFile::Any(list) => ClassReq::Any(
            list.into_iter()
                .map(|r| resolve_req(r, profs, titles))
                .collect::<Result<_, _>>()?,
        ),
        ClassReqFile::Title(t) => match titles.get(&t) {
            Some(&id) => ClassReq::Title(id),
            None => return Err(format!("title `{t}` not found")),
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDef {
    pub id: String,
    pub name: TextId,
    pub description: TextId,
    /// 1 to 4.
    pub tier: u8,
    /// The class this one specializes (tier 2+); `None` for tier 1.
    pub parent: Option<ClassId>,
    /// Level to earn it (: requirements are for earning only).
    pub level: u8,
    pub requires: Vec<ClassReq>,
    /// Points the class adds by itself at every new level.
    pub auto_points: Attributes,
    /// Proficiency gain multipliers in thousandths (absent: 1000).
    pub growth: Vec<(ProficiencyId, u32)>,
    /// Level XP of a craft, in thousandths of the recipe's XP (phase 8).
    /// 0: the economy default, which is almost nothing — the mark of a
    /// class that does not produce.
    pub craft_xp_permille: u32,
    /// Best rarity this class may craft
    pub quality_cap: Rarity,
    /// Icon name in `assets/icons/` (empty: generic).
    pub icon: String,
    /// Special attributes the class grows (faith for the paladin
    /// and the cleric), counted from the level it is earned.
    pub specials: Vec<ClassSpecial>,
    /// Techniques learned for good, at level 1, when the class
    /// is chosen (or later, once their requirement holds).
    pub techniques: Vec<TechniqueId>,
    /// Only one character of the server may have it at a time.
    pub unique: Option<Unique>,
}

/// A special attribute a class grows: `base` points when it is
/// earned and `per_level_permille` thousandths of a point for every level
/// reached above the class's own.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ClassSpecial {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::SpecialAttributeRef"))]
    pub special: String,
    #[serde(default)]
    pub base: u8,
    #[serde(default)]
    pub per_level_permille: u32,
}

impl ClassSpecial {
    /// Points at `max_level` for a class earned at `class_level`.
    pub fn points(&self, class_level: u8, max_level: u8) -> u32 {
        let above = u32::from(max_level.saturating_sub(class_level));
        u32::from(self.base).saturating_add(self.per_level_permille.saturating_mul(above) / 1000)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ClassFile {
    pub tier: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "editor", schemars(with = "Option<crate::editor::ClassRef>"))]
    pub parent: Option<String>,
    pub level: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<ClassReqFile>,
    #[serde(default)]
    pub auto_points: Attributes,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(
        feature = "editor",
        schemars(with = "Vec<(crate::editor::ProficiencyRef, u32)>")
    )]
    pub growth: Vec<(String, u32)>,
    #[serde(default)]
    pub craft_xp_permille: u32,
    #[serde(default)]
    pub quality_cap: Rarity,
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::IconRef"))]
    pub icon: String,
    /// Special attributes the class grows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub specials: Vec<ClassSpecial>,
    /// Techniques learned when the class is chosen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "editor", schemars(with = "Vec<crate::editor::TechniqueRef>"))]
    pub techniques: Vec<String>,
    /// Only one character of the server may have this class at a time (a
    /// legendary class); it passes to an heir when the holder stops playing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique: Option<Unique>,
}

pub(crate) fn parse_class(id: &str, text: &str) -> Result<ClassFile, String> {
    if !valid_id(id) {
        return Err(format!("class id `{id}` must be [a-z0-9_]"));
    }
    let f: ClassFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if !(1..=4).contains(&f.tier) || f.level == 0 {
        return Err("tier must be 1..=4 and level >= 1".into());
    }
    if (f.tier == 1) != f.parent.is_none() {
        return Err("tier 1 has no parent; tiers 2 to 4 must name one".into());
    }
    Ok(f)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum ProficiencyKind {
    Weapon,
    Defense,
    Magic,
    Production,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProficiencyDef {
    pub id: String,
    pub name: TextId,
    pub kind: ProficiencyKind,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ProficiencyFile {
    pub kind: ProficiencyKind,
}

pub(crate) fn parse_proficiency(id: &str, text: &str) -> Result<ProficiencyFile, String> {
    if !valid_id(id) {
        return Err(format!("proficiency id `{id}` must be [a-z0-9_]"));
    }
    ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))
}

/// The proficiency every character uses with no weapon.
pub const UNARMED: &str = "unarmed";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechniqueDef {
    pub id: String,
    pub name: TextId,
    pub description: TextId,
    pub proficiency: ProficiencyId,
    /// Proficiency points that unlock it.
    pub unlock_at: u8,
    pub cooldown_ticks: u32,
    pub vigor: u32,
    /// Spent besides the vigor (magic); 0 for most.
    pub mp: u32,
    /// Damage multiplier at level 1, in thousandths (0: not an attack).
    pub damage_permille: u32,
    /// Support: heals a standing player (or oneself) by these
    /// thousandths of their maximum HP at level 1.
    pub heal_permille: u32,
    /// Support: stands a fallen player up with these thousandths
    /// of their maximum HP.
    pub revive_permille: u32,
    /// Added per technique level above 1, in thousandths.
    pub growth_permille: u32,
    pub range: u16,
    /// Icon name in `assets/icons/` (empty: generic).
    pub icon: String,
    /// Support: a timed buff on the caster.
    pub buff: Option<Buff>,
    /// Special attributes needed to learn it (any way but a
    /// title or an NPC), checked when it would be learned; never to keep it.
    pub requires_special: Vec<(String, u8)>,
    /// Never unlocked by proficiency points, only taught (class,
    /// title, NPC).
    pub taught_only: bool,
    /// A blow that also hits every foe within this many tiles of the
    /// target (0: only the target).
    pub area: u16,
    /// What the blow leaves on whoever it hits.
    pub status: Option<Status>,
}

/// What a buff raises. New stats are appended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum BuffStat {
    /// Defense (armor), by thousandths of itself plus a floor of points.
    Defense,
}

/// A timed buff on the caster: `stat` raised by `permille`
/// thousandths (at technique level 1, plus `growth_permille` per level and
/// the effective proficiency points) and at least `flat` points, for
/// `duration_ticks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Buff {
    pub stat: BuffStat,
    pub permille: u32,
    #[serde(default)]
    pub flat: u32,
    pub duration_ticks: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct TechniqueFile {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ProficiencyRef"))]
    pub proficiency: String,
    pub unlock_at: u8,
    pub cooldown_ticks: u32,
    #[serde(default)]
    pub vigor: u32,
    #[serde(default)]
    pub mp: u32,
    #[serde(default)]
    pub damage_permille: u32,
    #[serde(default)]
    pub heal_permille: u32,
    #[serde(default)]
    pub revive_permille: u32,
    #[serde(default)]
    pub growth_permille: u32,
    #[serde(default = "one_tile")]
    pub range: u16,
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::IconRef"))]
    pub icon: String,
    /// A timed buff on the caster.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buff: Option<Buff>,
    /// Special attributes needed to learn it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(
        feature = "editor",
        schemars(with = "Vec<(crate::editor::SpecialAttributeRef, u8)>")
    )]
    pub requires_special: Vec<(String, u8)>,
    /// Only taught (class, title, NPC), never unlocked by points.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub taught_only: bool,
    /// A blow that also hits every foe within this many tiles of the
    /// target (0: only the target).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub area: u16,
    /// What the blow leaves on whoever it hits (poison, stun, slow).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

fn is_zero(v: &u16) -> bool {
    *v == 0
}

fn one_tile() -> u16 {
    1
}

pub(crate) fn parse_technique(id: &str, text: &str) -> Result<TechniqueFile, String> {
    if !valid_id(id) {
        return Err(format!("technique id `{id}` must be [a-z0-9_]"));
    }
    let f: TechniqueFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if f.range == 0 || f.cooldown_ticks == 0 {
        return Err("range and cooldown_ticks must be >= 1".into());
    }
    // One effect per technique: a blow, a heal, a revival or a
    // buff.
    let buff = f.buff.map_or(0, |b| b.permille.max(b.flat));
    let effects = [f.damage_permille, f.heal_permille, f.revive_permille, buff]
        .iter()
        .filter(|v| **v > 0)
        .count();
    if effects != 1 {
        return Err(
            "exactly one of damage_permille, heal_permille, revive_permille, buff must be above 0".into(),
        );
    }
    if f.buff.is_some_and(|b| b.duration_ticks == 0) {
        return Err("buff duration_ticks must be >= 1".into());
    }
    if f.revive_permille > 1000 || f.heal_permille > 1000 {
        return Err("heal_permille and revive_permille are at most 1000".into());
    }
    // Area and status ride on a blow.
    if (f.area > 0 || f.status.is_some()) && f.damage_permille == 0 {
        return Err("area and status need damage_permille above 0".into());
    }
    if f.area > MAX_AREA {
        return Err(format!("area is at most {MAX_AREA}"));
    }
    if let Some(s) = &f.status {
        check_status(s)?;
    }
    Ok(f)
}

/// What earns a title, with content ids resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleTrigger {
    /// Reach this level.
    Level(u8),
    /// A declared counter reaches `at`. `key` empty: the family total.
    Counter { family: String, key: String, at: u64 },
    /// A proficiency reaches these points.
    Proficiency(ProficiencyId, u8),
    /// A base attribute (points spent and given by level, without
    /// equipment) reaches this value.
    Attribute(Attribute, u16),
    /// Karma reaches this value: at least it when positive or zero, at
    /// most it when negative (a title for the cruel).
    Karma(i32),
}

/// What earns a title, as written in `titles/<id>.ron`. New triggers are
/// appended.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub(crate) enum TitleTriggerFile {
    /// Reach this level.
    Level(u8),
    /// A declared counter reaches `at`. `key` empty: the family total.
    Counter {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::CounterFamilyRef"))]
        family: String,
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::CounterKeyRef"))]
        key: String,
        at: u64,
    },
    /// A proficiency reaches these points.
    Proficiency(
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ProficiencyRef"))] String,
        u8,
    ),
    /// A base attribute (without equipment) reaches this value.
    Attribute(Attribute, u16),
    /// Karma reaches this value (negative: at most it).
    Karma(i32),
}

/// What a title gives while owned: every owned title counts, always
/// New bonuses are appended.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum TitleBonus {
    /// Added to the attribute in use (not the base one, so a title never
    /// earns another title or a class by itself).
    Attribute(Attribute, u16),
    /// Added to maximum HP.
    MaxHp(u32),
    /// Added to maximum vigor.
    MaxVigor(u32),
    /// Percent added to a rate.
    Percent(PercentBonus, u16),
    /// Points of a special attribute (0 to 100 in total).
    Special(
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::SpecialAttributeRef"))] String,
        u8,
    ),
    /// Phase 10: percent added to a rate while the character's level is at
    /// most the last number (the arrival title: +5% XP up to level 10).
    PercentUntilLevel(PercentBonus, u16, u8),
}

/// Rates a title can raise by a percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum PercentBonus {
    /// Damage dealt by any blow.
    Damage,
    /// XP from monsters.
    Xp,
    /// Maximum MP (P03: the arcane scholar's +3%).
    MaxMp,
}

/// Only one character of the server holds this at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Unique {
    /// Days without playing after which the holder loses it and another
    /// character may earn it (an heir). 0: kept for good.
    #[serde(default = "default_heir_days")]
    pub heir_after_days: u16,
}

fn default_heir_days() -> u16 {
    30
}

/// Who hears that a title was earned. New variants are appended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum Announce {
    /// Only the character (a System message).
    #[default]
    Quiet,
    /// The whole server, once: when the first character of the server
    /// earns it (a "server first", kept for good).
    First,
    /// The whole server, every time a character earns it.
    Everyone,
}

impl Announce {
    pub fn is_quiet(&self) -> bool {
        *self == Announce::Quiet
    }
}

/// Highest value of a special attribute (atributos.md).
pub const SPECIAL_MAX: u8 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleDef {
    pub id: String,
    pub name: TextId,
    pub description: TextId,
    pub trigger: TitleTrigger,
    /// Icon name in `assets/icons/` (empty: generic).
    pub icon: String,
    pub bonuses: Vec<TitleBonus>,
    /// Learned for good at level 1 when the title is earned.
    pub techniques: Vec<TechniqueId>,
    /// Titles this one replaces, directly or through a title it replaces
    /// (closed at validation), sorted. Earning it takes them away, and they
    /// are never earned again while it is owned.
    pub replaces: Vec<TitleId>,
    /// Who hears that it was earned.
    pub announce: Announce,
    /// Only one character of the server holds it at a time.
    pub unique: Option<Unique>,
}

/// An attribute earned by deeds (titles) or grown by a class.
/// What it does is a separate effect type, added when first used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecialAttributeDef {
    pub id: String,
    pub name: TextId,
    pub description: TextId,
    pub icon: String,
    /// Proficiencies it adds to, in thousandths of a point per
    /// point of the attribute (faith 40 at 250: light +10 in use).
    pub scales: Vec<(ProficiencyId, u32)>,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct SpecialAttributeFile {
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::IconRef"))]
    pub icon: String,
    /// Proficiencies it adds to, thousandths of a point per point.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(
        feature = "editor",
        schemars(with = "Vec<(crate::editor::ProficiencyRef, u32)>")
    )]
    pub scales: Vec<(String, u32)>,
}

pub(crate) fn parse_special_attribute(id: &str, text: &str) -> Result<SpecialAttributeFile, String> {
    if !valid_id(id) {
        return Err(format!("special attribute id `{id}` must be [a-z0-9_]"));
    }
    ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct TitleFile {
    pub trigger: TitleTriggerFile,
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::IconRef"))]
    pub icon: String,
    /// What the title gives. Every owned title counts, always; bonuses of
    /// different titles add up.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bonuses: Vec<TitleBonus>,
    /// Titles that go away when this one is earned (a candidate replaced
    /// by the real thing) and are never earned again while it is owned.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "editor", schemars(with = "Vec<crate::editor::TitleRef>"))]
    pub replaces: Vec<String>,
    /// Techniques learned for good, at level 1, when the title is earned.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "editor", schemars(with = "Vec<crate::editor::TechniqueRef>"))]
    pub techniques: Vec<String>,
    /// Who hears that it was earned: only the character (`Quiet`), the
    /// whole server the first time on the server (`First`), or every time
    /// (`Everyone`).
    #[serde(default, skip_serializing_if = "Announce::is_quiet")]
    pub announce: Announce,
    /// Only one character of the server holds it at a time; it passes to
    /// an heir when the holder stops playing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique: Option<Unique>,
}

pub(crate) fn parse_title(id: &str, text: &str) -> Result<TitleFile, String> {
    if !valid_id(id) {
        return Err(format!("title id `{id}` must be [a-z0-9_]"));
    }
    ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))
}

/// `counters.ron`: the families on. Unknown or repeated names are errors.
pub(crate) fn parse_counters(text: &str) -> Result<Vec<String>, String> {
    let list: Vec<String> = ron::from_str(text).map_err(|e| format!("counters.ron: {e}"))?;
    for (i, f) in list.iter().enumerate() {
        if !COUNTER_FAMILIES.contains(&f.as_str()) {
            return Err(format!(
                "counters.ron: unknown family `{f}` (known: {COUNTER_FAMILIES:?})"
            ));
        }
        if list.iter().take(i).any(|g| g == f) {
            return Err(format!("counters.ron: `{f}` repeated"));
        }
    }
    Ok(list)
}

pub fn class_name_key(id: &str) -> String {
    format!("class.{id}.name")
}
pub fn class_desc_key(id: &str) -> String {
    format!("class.{id}.desc")
}
pub fn proficiency_name_key(id: &str) -> String {
    format!("proficiency.{id}.name")
}
pub fn technique_name_key(id: &str) -> String {
    format!("technique.{id}.name")
}
pub fn technique_desc_key(id: &str) -> String {
    format!("technique.{id}.desc")
}
pub fn title_name_key(id: &str) -> String {
    format!("title.{id}.name")
}
pub fn title_desc_key(id: &str) -> String {
    format!("title.{id}.desc")
}
pub fn special_name_key(id: &str) -> String {
    format!("special.{id}.name")
}
pub fn special_desc_key(id: &str) -> String {
    format!("special.{id}.desc")
}

/// Ids resolved to make the defs (helper for `catalog`).
/// Loot lines of a monster or a shared table: a real chance and a real pile.
pub(crate) fn check_loot(loot: &[LootFile]) -> Result<(), String> {
    for l in loot {
        if l.chance == 0 || l.chance > 1000 || l.qty.0 == 0 || l.qty.0 > l.qty.1 {
            return Err(format!("loot `{}`: chance 1..=1000, qty 1 <= min <= max", l.item));
        }
    }
    Ok(())
}

pub(crate) fn resolve_loot(
    loot: Vec<LootFile>,
    items: &std::collections::BTreeMap<String, ItemId>,
) -> Result<Vec<LootDef>, String> {
    loot.into_iter()
        .map(|l| match items.get(&l.item) {
            Some(&item) => Ok(LootDef {
                item,
                chance: l.chance,
                qty: l.qty,
            }),
            None => Err(format!("loot item `{}` not found", l.item)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_checks() {
        let m = parse_monster("wolf", "(level: 2, hp: 30, damage: (2, 4), xp: 30)").unwrap();
        assert_eq!(m.attack_ticks, 30);
        assert!(parse_monster("wolf", "(level: 2, hp: 30, damage: (5, 4), xp: 30)").is_err());
        assert!(parse_class("warrior", "(tier: 1, level: 5)").is_ok());
        assert!(parse_class("knight", "(tier: 2, level: 10)").is_err());
        assert!(parse_counters(r#"["kills", "deaths"]"#).is_ok());
        assert!(parse_counters(r#"["kills", "kills"]"#).is_err());
        assert!(parse_counters(r#"["steps"]"#).is_err());
        let t = parse_title(
            "x",
            "(trigger: Counter(family: \"kills\", key: \"wolf\", at: 10))",
        )
        .unwrap();
        assert!(matches!(t.trigger, TitleTriggerFile::Counter { at: 10, .. }));
    }
}

#[cfg(test)]
mod technique_tests {
    use super::parse_technique;

    /// A technique does one thing: a blow, a heal or a revival.
    #[test]
    fn one_effect_per_technique() {
        assert!(
            parse_technique(
                "a",
                "(proficiency: \"x\", unlock_at: 1, cooldown_ticks: 1, damage_permille: 1000)"
            )
            .is_ok()
        );
        assert!(
            parse_technique(
                "h",
                "(proficiency: \"x\", unlock_at: 1, cooldown_ticks: 1, heal_permille: 150)"
            )
            .is_ok()
        );
        assert!(
            parse_technique(
                "r",
                "(proficiency: \"x\", unlock_at: 75, cooldown_ticks: 1, revive_permille: 500)"
            )
            .is_ok()
        );
        assert!(parse_technique("n", "(proficiency: \"x\", unlock_at: 1, cooldown_ticks: 1)").is_err());
        assert!(
            parse_technique(
                "b",
                "(proficiency: \"x\", unlock_at: 1, cooldown_ticks: 1, damage_permille: 1, heal_permille: 1)"
            )
            .is_err()
        );
        assert!(
            parse_technique(
                "o",
                "(proficiency: \"x\", unlock_at: 1, cooldown_ticks: 1, revive_permille: 1001)"
            )
            .is_err()
        );
    }
}
