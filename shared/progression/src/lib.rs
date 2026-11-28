//! XP, gain by use, requirements, classes and titles
//!
//! Pure: the sheet goes
//! in, the new sheet and a list of [`Change`] come out; the `sim` turns
//! changes into System messages. Requirements are for earning, never for
//! keeping.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
#![deny(clippy::float_arithmetic)]
// Overflow and division by zero never panic in a tick; tests may use plain arithmetic.
#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use base::{Attribute, ClassId, ProficiencyId, TechniqueId, TitleId};
use defs::{Attributes, ClassReq, PercentBonus, SPECIAL_MAX, TitleBonus, TitleTrigger, ValidatedCatalog};
use serde::{Deserialize, Serialize};

/// Ticks between two gains of the same proficiency (0.5 s, anti macro).
pub const MIN_GAIN_INTERVAL_TICKS: u64 = 10;
/// Base proficiency XP of one effective use.
pub const USE_XP: u64 = 10;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proficiency {
    pub id: ProficiencyId,
    pub points: u8,
    /// XP toward the next point.
    pub xp: u64,
    /// Tick of the last gain (anti macro).
    pub last_gain: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Technique {
    pub id: TechniqueId,
    pub level: u8,
    /// Effective uses toward the next level.
    pub uses: u64,
}

/// Everything about how a character grew.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressionSheet {
    pub level: u8,
    /// XP inside the current level.
    pub xp: u64,
    /// Highest level ever reached: only levels above it give points.
    pub max_level: u8,
    /// Distributed attributes (start value + free points spent + class points).
    pub attributes: Attributes,
    pub free_points: u16,
    pub class: Option<ClassId>,
    /// Sorted by id.
    pub proficiencies: Vec<Proficiency>,
    /// Sorted by id.
    pub techniques: Vec<Technique>,
    /// Sorted.
    pub titles: Vec<TitleId>,
    /// Hidden morality, `-KARMA_MAX..=KARMA_MAX`; never shown to the player.
    pub karma: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Change {
    LeveledUp(u8),
    LevelLost(u8),
    PointsGained(u16),
    ProficiencyRaised(ProficiencyId, u8),
    TechniqueUnlocked(TechniqueId),
    TechniqueRaised(TechniqueId, u8),
    TitleEarned(TitleId),
    /// Classes the character may now choose (offered once per level reached).
    ClassesAvailable(Vec<ClassId>),
    ClassChosen(ClassId),
    /// Earned `new`, which took `old` away. Appended last.
    TitleReplaced {
        old: TitleId,
        new: TitleId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressError {
    /// More points than the sheet has, or none.
    NotEnoughPoints,
    /// Unknown class, requirements not met, or not the next step of the tree.
    ClassNotAvailable,
}

impl ProgressionSheet {
    pub fn new(cat: &ValidatedCatalog) -> ProgressionSheet {
        ProgressionSheet {
            level: 1,
            xp: 0,
            max_level: 1,
            attributes: Attributes::all(cat.formulas().progression.start_attribute),
            free_points: 0,
            class: None,
            proficiencies: Vec::new(),
            techniques: Vec::new(),
            titles: Vec::new(),
            karma: 0,
        }
    }

    /// Adds to karma, kept inside its range.
    pub fn change_karma(&mut self, delta: i32) {
        self.karma = self
            .karma
            .saturating_add(delta)
            .clamp(-defs::KARMA_MAX, defs::KARMA_MAX);
    }

    pub fn proficiency(&self, id: ProficiencyId) -> u8 {
        self.proficiencies
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .and_then(|i| self.proficiencies.get(i))
            .map_or(0, |p| p.points)
    }

    pub fn technique(&self, id: TechniqueId) -> Option<&Technique> {
        self.techniques
            .binary_search_by_key(&id, |t| t.id)
            .ok()
            .and_then(|i| self.techniques.get(i))
    }
}

/// Adds level XP; levels up as many times as it covers. Points only above
/// `max_level`.
pub fn add_xp(s: &mut ProgressionSheet, amount: u64, cat: &ValidatedCatalog) -> Vec<Change> {
    let f = cat.formulas();
    let mut changes = Vec::new();
    s.xp = s.xp.saturating_add(amount);
    while s.level < f.progression.max_level && s.xp >= f.level_xp(s.level) {
        s.xp = s.xp.saturating_sub(f.level_xp(s.level));
        s.level = s.level.saturating_add(1);
        changes.push(Change::LeveledUp(s.level));
        if s.level > s.max_level {
            s.max_level = s.level;
            let class = s.class.and_then(|c| cat.class(c));
            let free = match class {
                Some(_) => f.progression.free_points,
                None => f.progression.free_points_without_class,
            };
            s.free_points = s.free_points.saturating_add(free);
            changes.push(Change::PointsGained(free));
            if let Some(c) = class {
                s.attributes = s.attributes.plus(&c.auto_points);
            }
            let offer = available_classes(s, cat);
            if !offer.is_empty() {
                changes.push(Change::ClassesAvailable(offer));
            }
            // The class's special attributes grew: a technique may be due.
            changes.extend(teach_class_techniques(s, cat));
        }
    }
    if s.level >= f.progression.max_level {
        s.xp = 0;
    }
    changes
}

/// Death: one level down, XP at the start of the new level.
/// Attributes, class, techniques, proficiencies and titles stay.
pub fn lose_level(s: &mut ProgressionSheet) -> Vec<Change> {
    s.xp = 0;
    if s.level > 1 {
        s.level = s.level.saturating_sub(1);
        return vec![Change::LevelLost(s.level)];
    }
    Vec::new()
}

/// Spends free points. Permanent: no way back.
pub fn allocate(s: &mut ProgressionSheet, points: &Attributes) -> Result<(), ProgressError> {
    let total = points.total();
    if total == 0 || total > u32::from(s.free_points) {
        return Err(ProgressError::NotEnoughPoints);
    }
    s.attributes = s.attributes.plus(points);
    s.free_points = s
        .free_points
        .saturating_sub(u16::try_from(total).unwrap_or(u16::MAX));
    Ok(())
}

/// Gain for the target's level against the proficiency: nothing 10+ levels
/// below, up to ×1.5 above (thousandths).
pub fn difficulty_factor(target_level: u8, points: u8) -> u64 {
    let equivalent = i64::from(points.max(1));
    let gap = i64::from(target_level).saturating_sub(equivalent);
    let factor = if gap <= -10 {
        0
    } else if gap < 0 {
        // -9..-1: fades from 1000 down to 100.
        gap.saturating_mul(100).saturating_add(1000)
    } else {
        gap.min(10).saturating_mul(50).saturating_add(1000)
    };
    u64::try_from(factor).unwrap_or(0)
}

/// One effective use of a proficiency (a blow that landed). Raises points
/// and unlocks the techniques that ask for them.
pub fn use_proficiency(
    s: &mut ProgressionSheet,
    id: ProficiencyId,
    target_level: u8,
    tick: u64,
    cat: &ValidatedCatalog,
) -> Vec<Change> {
    let f = cat.formulas();
    let mult = s
        .class
        .and_then(|c| cat.class(c))
        .and_then(|c| {
            c.growth
                .iter()
                .find(|(p, _)| *p == id)
                .map(|(_, m)| u64::from(*m))
        })
        .unwrap_or(1000);
    let i = match s.proficiencies.binary_search_by_key(&id, |p| p.id) {
        Ok(i) => i,
        Err(i) => {
            s.proficiencies.insert(
                i,
                Proficiency {
                    id,
                    points: 0,
                    xp: 0,
                    last_gain: 0,
                },
            );
            i
        }
    };
    let mut changes = Vec::new();
    let Some(p) = s.proficiencies.get_mut(i) else {
        return changes;
    };
    if p.last_gain != 0 && tick.saturating_sub(p.last_gain) < MIN_GAIN_INTERVAL_TICKS {
        return changes;
    }
    p.last_gain = tick.max(1);
    let gain = USE_XP.saturating_mul(difficulty_factor(target_level, p.points)) / 1000;
    let gain = gain.saturating_mul(mult) / 1000;
    p.xp = p.xp.saturating_add(gain);
    let mut raised = false;
    while p.points < f.progression.prof_max && p.xp >= f.prof_xp(p.points) {
        p.xp = p.xp.saturating_sub(f.prof_xp(p.points));
        p.points = p.points.saturating_add(1);
        raised = true;
    }
    if raised {
        let points = p.points;
        changes.push(Change::ProficiencyRaised(id, points));
        for (tid, t) in cat.techniques() {
            if t.proficiency == id
                && t.unlock_at <= points
                && !t.taught_only
                && s.technique(tid).is_none()
                && t.requires_special
                    .iter()
                    .all(|(sp, v)| special_attribute(s, sp, cat) >= *v)
            {
                let at = s.techniques.partition_point(|x| x.id < tid);
                s.techniques.insert(
                    at,
                    Technique {
                        id: tid,
                        level: 1,
                        uses: 0,
                    },
                );
                changes.push(Change::TechniqueUnlocked(tid));
            }
        }
    }
    changes
}

/// A technique taught (: an NPC teaches the first magic), at
/// level 1; `None` when it was already known.
pub fn learn_technique(s: &mut ProgressionSheet, id: TechniqueId) -> Option<Change> {
    if s.technique(id).is_some() {
        return None;
    }
    let at = s.techniques.partition_point(|x| x.id < id);
    s.techniques.insert(
        at,
        Technique {
            id,
            level: 1,
            uses: 0,
        },
    );
    Some(Change::TechniqueUnlocked(id))
}

/// One effective use of a technique.
pub fn use_technique(s: &mut ProgressionSheet, id: TechniqueId, cat: &ValidatedCatalog) -> Vec<Change> {
    let f = cat.formulas();
    let Some(t) = s
        .techniques
        .binary_search_by_key(&id, |t| t.id)
        .ok()
        .and_then(|i| s.techniques.get_mut(i))
    else {
        return Vec::new();
    };
    t.uses = t.uses.saturating_add(1);
    if t.uses >= f.technique_uses(t.level) {
        t.uses = 0;
        t.level = t.level.saturating_add(1);
        return vec![Change::TechniqueRaised(id, t.level)];
    }
    Vec::new()
}

fn holds(r: &ClassReq, s: &ProgressionSheet) -> bool {
    match r {
        ClassReq::Attribute(a, v) => s.attributes.get(*a) >= *v,
        ClassReq::Proficiency(p, v) => s.proficiency(*p) >= *v,
        ClassReq::Any(list) => list.iter().any(|r| holds(r, s)),
        ClassReq::Title(t) => s.titles.binary_search(t).is_ok(),
    }
}

/// Classes the sheet may choose now: the next tier of its tree (tier 1 with
/// no class), with every requirement met at the current level.
pub fn available_classes(s: &ProgressionSheet, cat: &ValidatedCatalog) -> Vec<ClassId> {
    available_classes_except(s, cat, &[])
}

/// [`available_classes`] without `taken`: unique classes another character
/// of the server holds.
pub fn available_classes_except(
    s: &ProgressionSheet,
    cat: &ValidatedCatalog,
    taken: &[ClassId],
) -> Vec<ClassId> {
    cat.classes()
        .filter(|(id, _)| !taken.contains(id))
        .filter(|(_, c)| c.parent == s.class)
        .filter(|(_, c)| s.level >= c.level && c.requires.iter().all(|r| holds(r, s)))
        .map(|(id, _)| id)
        .collect()
}

/// Takes a class offered by [`available_classes`]. Permanent.
pub fn choose_class(
    s: &mut ProgressionSheet,
    class: ClassId,
    cat: &ValidatedCatalog,
) -> Result<Change, ProgressError> {
    choose_class_except(s, class, cat, &[])
}

/// [`choose_class`] refusing the unique classes in `taken`.
pub fn choose_class_except(
    s: &mut ProgressionSheet,
    class: ClassId,
    cat: &ValidatedCatalog,
    taken: &[ClassId],
) -> Result<Change, ProgressError> {
    if !available_classes_except(s, cat, taken).contains(&class) {
        return Err(ProgressError::ClassNotAvailable);
    }
    s.class = Some(class);
    Ok(Change::ClassChosen(class))
}

/// [`choose_class`] and the techniques the class teaches.
pub fn choose_class_and_learn(
    s: &mut ProgressionSheet,
    class: ClassId,
    cat: &ValidatedCatalog,
) -> Result<Vec<Change>, ProgressError> {
    choose_class_and_learn_except(s, class, cat, &[])
}

/// [`choose_class_and_learn`] refusing the unique classes in `taken`.
pub fn choose_class_and_learn_except(
    s: &mut ProgressionSheet,
    class: ClassId,
    cat: &ValidatedCatalog,
    taken: &[ClassId],
) -> Result<Vec<Change>, ProgressError> {
    let mut changes = vec![choose_class_except(s, class, cat, taken)?];
    changes.extend(teach_class_techniques(s, cat));
    Ok(changes)
}

/// Titles whose trigger holds and the sheet lacks. `counter(family, key)`
/// reads the character's declared counters (key empty: family total).
pub fn check_titles(
    s: &mut ProgressionSheet,
    counter: impl Fn(&str, &str) -> u64,
    cat: &ValidatedCatalog,
) -> Vec<Change> {
    check_titles_except(s, counter, cat, &[])
}

/// [`check_titles`] leaving out `taken`: unique titles another character of
/// the server holds.
pub fn check_titles_except(
    s: &mut ProgressionSheet,
    counter: impl Fn(&str, &str) -> u64,
    cat: &ValidatedCatalog,
    taken: &[TitleId],
) -> Vec<Change> {
    let mut changes = Vec::new();
    // A title is out of reach while an owned title replaces it.
    let replaced = |owned: &[TitleId], id: TitleId| {
        owned
            .iter()
            .filter_map(|&o| cat.title(o))
            .any(|o| o.replaces.binary_search(&id).is_ok())
    };
    let mut earned: Vec<TitleId> = Vec::new();
    for (id, t) in cat.titles() {
        if s.titles.binary_search(&id).is_ok() || replaced(&s.titles, id) || taken.contains(&id) {
            continue;
        }
        let ok = match &t.trigger {
            TitleTrigger::Level(l) => s.level >= *l,
            TitleTrigger::Counter { family, key, at } => counter(family, key) >= *at,
            TitleTrigger::Proficiency(p, v) => s.proficiency(*p) >= *v,
            TitleTrigger::Attribute(a, v) => s.attributes.get(*a) >= *v,
            TitleTrigger::Karma(v) if *v < 0 => s.karma <= *v,
            TitleTrigger::Karma(v) => s.karma >= *v,
        };
        if ok {
            earned.push(id);
        }
    }
    // Earned together with the title that replaces it: never given.
    let all = earned.clone();
    earned.retain(|&id| !replaced(&all, id));
    for id in earned {
        let Some(t) = cat.title(id) else { continue };
        let at = s.titles.partition_point(|x| *x < id);
        s.titles.insert(at, id);
        let gone: Vec<TitleId> = s
            .titles
            .iter()
            .copied()
            .filter(|o| t.replaces.binary_search(o).is_ok())
            .collect();
        s.titles.retain(|o| t.replaces.binary_search(o).is_err());
        match gone.first() {
            Some(&old) => changes.push(Change::TitleReplaced { old, new: id }),
            None => changes.push(Change::TitleEarned(id)),
        }
        // A technique a title teaches is kept for good.
        for &tid in &t.techniques {
            if s.technique(tid).is_none() {
                let at = s.techniques.partition_point(|x| x.id < tid);
                s.techniques.insert(
                    at,
                    Technique {
                        id: tid,
                        level: 1,
                        uses: 0,
                    },
                );
                changes.push(Change::TechniqueUnlocked(tid));
            }
        }
    }
    changes
}

/// What every owned title adds, summed. Special attributes are
/// read one at a time with [`special_attribute`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TitleBonuses {
    pub attributes: Attributes,
    pub max_hp: u32,
    pub max_vigor: u32,
    pub damage_percent: u32,
    pub xp_percent: u32,
    pub max_mp_percent: u32,
    /// Seconds between two last stands, the shortest of the titles; `None`:
    /// no title gives one.
    pub last_stand_secs: Option<u32>,
}

impl TitleBonuses {
    fn add_percent(&mut self, kind: PercentBonus, v: u16) {
        let slot = match kind {
            PercentBonus::Damage => &mut self.damage_percent,
            PercentBonus::Xp => &mut self.xp_percent,
            PercentBonus::MaxMp => &mut self.max_mp_percent,
        };
        *slot = slot.saturating_add(u32::from(v));
    }

    /// `value` raised by `percent`, rounded down.
    pub fn raise(value: u64, percent: u32) -> u64 {
        value.saturating_mul(100 + u64::from(percent)) / 100
    }
}

pub fn title_bonuses(s: &ProgressionSheet, cat: &ValidatedCatalog) -> TitleBonuses {
    let mut b = TitleBonuses::default();
    for t in s.titles.iter().filter_map(|&id| cat.title(id)) {
        for bonus in &t.bonuses {
            match bonus {
                TitleBonus::Attribute(a, v) => {
                    let x = b.attributes.get_mut(*a);
                    *x = x.saturating_add(*v);
                }
                TitleBonus::MaxHp(v) => b.max_hp = b.max_hp.saturating_add(*v),
                TitleBonus::MaxVigor(v) => b.max_vigor = b.max_vigor.saturating_add(*v),
                TitleBonus::Percent(kind, v) => b.add_percent(*kind, *v),
                TitleBonus::PercentUntilLevel(kind, v, until) if s.level <= *until => {
                    b.add_percent(*kind, *v)
                }
                TitleBonus::LastStand(secs) => {
                    b.last_stand_secs = Some(b.last_stand_secs.map_or(*secs, |s| s.min(*secs)));
                }
                TitleBonus::Special(..) | TitleBonus::PercentUntilLevel(..) => {}
            }
        }
    }
    b
}

/// A special attribute: the sum of the owned titles that give it and of
/// what the class and its parents grow (by the highest level
/// reached), up to `SPECIAL_MAX` (atributos.md).
pub fn special_attribute(s: &ProgressionSheet, id: &str, cat: &ValidatedCatalog) -> u8 {
    let mut total: u32 = 0;
    for t in s.titles.iter().filter_map(|&tid| cat.title(tid)) {
        for bonus in &t.bonuses {
            if let TitleBonus::Special(s, v) = bonus
                && s == id
            {
                total = total.saturating_add(u32::from(*v));
            }
        }
    }
    let mut class = s.class.and_then(|c| cat.class(c));
    while let Some(c) = class {
        for sp in c.specials.iter().filter(|sp| sp.special == id) {
            total = total.saturating_add(sp.points(c.level, s.max_level));
        }
        class = c.parent.and_then(|p| cat.class(p));
    }
    u8::try_from(total.min(u32::from(SPECIAL_MAX))).unwrap_or(SPECIAL_MAX)
}

/// Every special attribute above 0, in catalog order (status window).
pub fn special_attributes(s: &ProgressionSheet, cat: &ValidatedCatalog) -> Vec<(usize, u8)> {
    cat.special_attributes()
        .iter()
        .enumerate()
        .map(|(i, d)| (i, special_attribute(s, &d.id, cat)))
        .filter(|(_, v)| *v > 0)
        .collect()
}

/// Points of a proficiency in use: the sheet's plus what special
/// attributes add (: faith adds to light), up to `prof_max`.
/// Never used to raise, unlock or earn anything.
pub fn effective_proficiency(s: &ProgressionSheet, id: ProficiencyId, cat: &ValidatedCatalog) -> u8 {
    let mut extra: u32 = 0;
    for d in cat.special_attributes() {
        for (_, k) in d.scales.iter().filter(|(p, _)| *p == id) {
            extra =
                extra.saturating_add(u32::from(special_attribute(s, &d.id, cat)).saturating_mul(*k) / 1000);
        }
    }
    let max = u32::from(cat.formulas().progression.prof_max);
    u8::try_from(u32::from(s.proficiency(id)).saturating_add(extra).min(max)).unwrap_or(u8::MAX)
}

/// The technique's special attributes hold.
pub fn may_learn(s: &ProgressionSheet, id: TechniqueId, cat: &ValidatedCatalog) -> bool {
    cat.technique(id).is_some_and(|t| {
        t.requires_special
            .iter()
            .all(|(sp, v)| special_attribute(s, sp, cat) >= *v)
    })
}

/// The class's techniques (and its parents') not known yet whose
/// requirement holds, learned at level 1. Run when the class is chosen and
/// at every new level.
pub fn teach_class_techniques(s: &mut ProgressionSheet, cat: &ValidatedCatalog) -> Vec<Change> {
    let mut wanted = Vec::new();
    let mut class = s.class.and_then(|c| cat.class(c));
    while let Some(c) = class {
        wanted.extend(c.techniques.iter().copied());
        class = c.parent.and_then(|p| cat.class(p));
    }
    let mut changes = Vec::new();
    for tid in wanted {
        if s.technique(tid).is_none() && may_learn(s, tid, cat) {
            changes.extend(learn_technique(s, tid));
        }
    }
    changes
}

/// All attributes of the sheet plus equipment or title bonuses.
pub fn attribute(s: &ProgressionSheet, a: Attribute, bonus: &Attributes) -> u16 {
    s.attributes.get(a).saturating_add(bonus.get(a))
}

#[cfg(test)]
mod tests;
