//! Combat, monsters, regeneration and death (phase 4).
//!
//! Order inside the tick: `ai` (monsters pick, chase and hit), `attacks`
//! (players' weapons on their target), `regen`, `fallen` (death after the
//! wait), `respawn`. Every blow goes through [`strike`], which applies the
//! `combat` formulas, threat, retaliation, proficiency by use and kills.

use crate::Zone;
use crate::gear::{Gear, ItemInstance, LOW_ITEM_IDS};
use crate::growth::Growth;
use crate::player::Slot;
use crate::types::{CombatOut, EquipSlot, Refusal, TickOutput};
use crate::zone::{Body, PlayerComp, Walk};
use base::{Arg, Attribute, EntityId, MonsterId, PlayerId, ProficiencyId, TextRef, TilePos};
use combat::{CombatSheet, HitResult, ThreatTable};
use defs::{Equip, RegionKind};
use progression::{Change, TitleBonuses};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::VecDeque;

/// Ticks between path recalculations while chasing.
const REPATH_TICKS: u64 = 10;
/// A move or technique this recent keeps an idle player from retaliating
/// (2 s, `combate-automatico.md`).
const RETALIATE_AFTER_TICKS: u64 = 40;

/// System messages that happen all the time: the log is enough, no window.
const LOG_ONLY: &[&str] = &["system.xp", "system.proficiency_up", "system.technique_up"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Fighter {
    pub hp: u32,
    pub max_hp: u32,
    pub vigor: u32,
    pub max_vigor: u32,
    pub mp: u32,
    pub max_mp: u32,
    /// MP regained per regeneration interval.
    pub mp_regen: u32,
    pub level: u8,
    pub target: Option<EntityId>,
    pub next_attack: u64,
    /// Global cooldown between techniques.
    pub gcd_until: u64,
    pub last_combat: u64,
    /// Fallen until this tick (players; 0: standing).
    pub fallen_until: u64,
    pub last_path: u64,
    /// Players: tick of the last move, stop or technique.
    pub last_order: u64,
    /// The target came from retaliation: hit it in reach, never chase.
    pub retaliating: bool,
    /// A defense buff (a vow) while `now < ward.until`.
    pub ward: Ward,
    /// Poison, stun and slow on this fighter, sorted by kind.
    #[serde(default)]
    pub statuses: Vec<crate::status::ActiveStatus>,
}

/// A timed defense buff: the defense rises by `permille`
/// thousandths of itself, at least `flat` points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ward {
    pub permille: u32,
    pub flat: u32,
    pub until: u64,
}

impl Ward {
    /// `base` defense with the buff, if it still holds at `now`.
    pub fn defense(&self, base: u32, now: u64) -> u32 {
        if now >= self.until {
            return base;
        }
        let share = u32::try_from(combat::permille(u64::from(base), self.permille)).unwrap_or(u32::MAX);
        base.saturating_add(share.max(self.flat))
    }
}

impl Fighter {
    pub fn new(hp: u32, vigor: u32, level: u8) -> Fighter {
        Fighter {
            hp,
            max_hp: hp,
            vigor,
            max_vigor: vigor,
            mp: 0,
            max_mp: 0,
            mp_regen: 0,
            level,
            target: None,
            next_attack: 0,
            gcd_until: 0,
            last_combat: 0,
            fallen_until: 0,
            last_path: 0,
            last_order: 0,
            retaliating: false,
            ward: Ward::default(),
            statuses: Vec::new(),
        }
    }

    pub fn alive(&self) -> bool {
        self.hp > 0 && self.fallen_until == 0
    }

    pub fn health(&self) -> u8 {
        if self.max_hp == 0 {
            return 100;
        }
        let pct = u64::from(self.hp)
            .saturating_mul(100)
            .checked_div(u64::from(self.max_hp))
            .unwrap_or(100);
        u8::try_from(pct).unwrap_or(100)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MonsterComp {
    pub monster: MonsterId,
    /// Entry of `monsters.ron` that placed it (respawn).
    pub spawn: u16,
    pub home: TilePos,
    pub threat: ThreatTable<EntityId>,
    /// Phase of the fight reached (1-based, 0: none); back to 0 on reset.
    #[serde(default)]
    pub phase: u8,
}

/// What a fighter brings to a blow besides the sheet.
/// Ticks until the next blow: a weapon that costs vigor takes it, and
/// without enough the blow waits twice as long.
pub(crate) fn blow_wait(f: &mut Fighter, attack_ticks: u32, vigor: u32) -> u64 {
    if f.vigor >= vigor {
        f.vigor = f.vigor.saturating_sub(vigor);
        u64::from(attack_ticks)
    } else {
        u64::from(attack_ticks) * 2
    }
}

struct Arms {
    sheet: CombatSheet,
    range: u16,
    attack_ticks: u32,
    prof: Option<ProficiencyId>,
    /// Vigor each blow costs.
    vigor: u32,
}

fn arms(z: &Zone, h: hecs::Entity) -> Option<Arms> {
    let cat = &z.content.catalog;
    let f = cat.formulas();
    let (level, ward) = z.world.get::<&Fighter>(h).ok().map(|f| (f.level, f.ward))?;
    if let Ok(m) = z.world.get::<&MonsterComp>(h) {
        let def = cat.monster(m.monster)?;
        return Some(Arms {
            sheet: CombatSheet {
                level,
                attributes: def.attributes,
                weapon_damage: def.damage,
                weapon_attribute: Attribute::Str,
                weapon_prof: def.weapon_prof,
                defense: def.defense,
            },
            range: def.range,
            attack_ticks: def.attack_ticks,
            prof: None,
            vigor: 0,
        });
    }
    let p = z.world.get::<&PlayerComp>(h).ok()?;
    let gear = z.world.get::<&Gear>(h).ok()?;
    let attributes = p.growth.attributes(&gear.bonus(), cat);
    let unarmed = cat.proficiency_by_name(defs::UNARMED).map(|(id, _)| id);
    let (damage, attribute, prof, range, ticks, vigor) = match gear.equip_def(EquipSlot::Weapon, cat) {
        Some(Equip::Weapon {
            damage,
            attack_ticks,
            attribute,
            proficiency,
            range,
            vigor,
            ..
        }) => {
            // The piece's rarity adds to both ends of the damage.
            let k = gear.weapon_permille(cat).saturating_add(1000);
            let up = |d: u32| u32::try_from(combat::permille(u64::from(d), k)).unwrap_or(u32::MAX);
            (
                (up(damage.0), up(damage.1)),
                *attribute,
                Some(*proficiency),
                *range,
                *attack_ticks,
                *vigor,
            )
        }
        _ => (
            f.combat.unarmed_damage,
            Attribute::Str,
            unarmed,
            1,
            f.combat.unarmed_attack_ticks,
            0,
        ),
    };
    Some(Arms {
        sheet: CombatSheet {
            level,
            attributes,
            weapon_damage: damage,
            weapon_attribute: attribute,
            weapon_prof: prof.map_or(0, |id| {
                progression::effective_proficiency(&p.growth.sheet, id, cat)
            }),
            defense: ward.defense(gear.defense(cat), z.tick.0),
        },
        range,
        attack_ticks: ticks,
        prof,
        vigor,
    })
}

/// The proficiency of what the entity holds in hand: the weapon's, or
/// `unarmed` with none. A technique needs it to match its own.
pub(crate) fn wielded(z: &Zone, h: hecs::Entity) -> Option<ProficiencyId> {
    arms(z, h)?.prof
}

impl Zone {
    pub(crate) fn region(&self, p: TilePos) -> Option<RegionKind> {
        defs::region_of(&self.regions, p)
    }

    pub(crate) fn in_workshop(&self, p: TilePos) -> bool {
        defs::in_workshop(&self.regions, p)
    }

    pub(crate) fn text(&self, key: &str, args: SmallVec<[Arg; 2]>) -> Option<TextRef> {
        let id = self.content.catalog.keys().id(key)?;
        Some(TextRef { id, args })
    }

    /// A System message for the System window; routine ones (`LOG_ONLY`)
    /// go only to the log.
    pub(crate) fn tell(&self, out: &mut TickOutput, player: PlayerId, key: &str, args: SmallVec<[Arg; 2]>) {
        if LOG_ONLY.contains(&key) {
            return self.note(out, player, key, args);
        }
        if let Some(t) = self.text(key, args) {
            out.system.push((player, t));
        }
    }

    /// A System message for the log only.
    pub(crate) fn note(&self, out: &mut TickOutput, player: PlayerId, key: &str, args: SmallVec<[Arg; 2]>) {
        if let Some(t) = self.text(key, args) {
            out.notes.push((player, t));
        }
    }

    /// Can `att` hit `tgt` from where they stand, with this reach?
    pub(crate) fn can_hit(&self, att: EntityId, tgt: EntityId, range: u16) -> Result<(), Refusal> {
        let (Some((ah, apos)), Some((th, tpos))) = (self.body_of(att), self.body_of(tgt)) else {
            return Err(Refusal::InvalidTarget);
        };
        if att == tgt {
            return Err(Refusal::InvalidTarget);
        }
        let alive = self.world.get::<&Fighter>(th).map(|f| f.alive()).unwrap_or(false);
        if !alive {
            return Err(Refusal::InvalidTarget);
        }
        // A player on safe ground who picks a monster outside it walks out
        // first (alpha: clicking from a camp or a town gate did nothing);
        // the blow itself still waits for both to be out.
        if self.region(apos) == Some(RegionKind::Safe)
            && self.region(tpos) != Some(RegionKind::Safe)
            && self.world.get::<&PlayerComp>(ah).is_ok()
            && self.world.get::<&MonsterComp>(th).is_ok()
        {
            return Err(Refusal::TooFar);
        }
        // No combat in safe ground; players fight players only in PvP ground.
        if self.region(apos) == Some(RegionKind::Safe) || self.region(tpos) == Some(RegionKind::Safe) {
            return Err(Refusal::InvalidTarget);
        }
        let both_players =
            self.world.get::<&PlayerComp>(ah).is_ok() && self.world.get::<&PlayerComp>(th).is_ok();
        if both_players
            && (self.region(apos) != Some(RegionKind::Pvp) || self.region(tpos) != Some(RegionKind::Pvp))
        {
            return Err(Refusal::InvalidTarget);
        }
        let both_monsters =
            self.world.get::<&MonsterComp>(ah).is_ok() && self.world.get::<&MonsterComp>(th).is_ok();
        if both_monsters || self.world.get::<&Fighter>(ah).is_err() {
            return Err(Refusal::InvalidTarget);
        }
        let d = apos.distance(tpos);
        if d > u32::from(range) || (d > 1 && !world::line_of_sight(&self.map, apos, tpos)) {
            return Err(Refusal::TooFar);
        }
        Ok(())
    }
}

/// One blow from `att` on `tgt` with a damage multiplier in thousandths,
/// by `technique` (`None`: the automatic attack). Returns whether it landed.
pub(crate) fn strike(
    z: &mut Zone,
    att: EntityId,
    tgt: EntityId,
    mult: u32,
    technique: Option<base::TechniqueId>,
    out: &mut TickOutput,
) -> bool {
    let (Some(&ah), Some(&th)) = (z.by_id.get(&att), z.by_id.get(&tgt)) else {
        return false;
    };
    let (Some(a), Some(d)) = (arms(z, ah), arms(z, th)) else {
        return false;
    };
    let pvp = z.world.get::<&PlayerComp>(ah).is_ok() && z.world.get::<&PlayerComp>(th).is_ok();
    let content = z.content.clone();
    let cat = &content.catalog;
    let mult = match z.world.get::<&PlayerComp>(ah) {
        Ok(p) => {
            let percent = progression::title_bonuses(&p.growth.sheet, cat).damage_percent;
            u32::try_from(TitleBonuses::raise(u64::from(mult), percent)).unwrap_or(u32::MAX)
        }
        Err(_) => mult,
    };
    let result = combat::resolve_hit(&cat.formulas().combat, &a.sheet, &d.sheet, mult, pvp, &mut z.rng);
    let now = z.tick.0;
    if let Ok(f) = z.world.query_one_mut::<&mut Fighter>(ah) {
        f.last_combat = now;
    }
    let (damage, crit) = match result {
        HitResult::Missed => (None, false),
        HitResult::Hit { damage, crit } => (Some(damage), crit),
    };
    let mut killed = false;
    if let Ok((f, body)) = z.world.query_one_mut::<(&mut Fighter, &mut Body)>(th) {
        f.last_combat = now;
        if let Some(dmg) = damage {
            f.hp = f.hp.saturating_sub(dmg);
            body.status_tick = now;
            killed = f.hp == 0;
        }
    }
    out.combat.push(CombatOut {
        attacker: att,
        target: tgt,
        damage,
        crit,
        killed,
        technique,
    });
    // Threat on monsters; retaliation of idle players (combate-automatico.md).
    if let Ok(m) = z.world.query_one_mut::<&mut MonsterComp>(th) {
        m.threat.add(att, u64::from(damage.unwrap_or(0)) + 1);
    }
    if let Ok((f, walk, p)) = z
        .world
        .query_one_mut::<(&mut Fighter, &Walk, &mut PlayerComp)>(th)
    {
        p.dirty_stats = true;
        let fleeing = !walk.steps.is_empty() || now.saturating_sub(f.last_order) < RETALIATE_AFTER_TICKS;
        if f.target.is_none() && !fleeing && z.by_id.contains_key(&att) {
            f.target = Some(att);
            f.retaliating = true;
        }
    }
    // Equipment wears out with use (phase 7): the attacker's weapon on a
    // blow that landed, the target's armor on a blow it took.
    if damage.is_some() {
        wear(z, ah, EquipSlot::Weapon, out);
        wear(z, th, EquipSlot::Armor, out);
    }
    // Proficiency by use: only blows that landed count.
    if damage.is_some()
        && let Some(prof) = a.prof
        && let Ok(p) = z.world.query_one_mut::<&mut PlayerComp>(ah)
    {
        let mut changes = progression::use_proficiency(&mut p.growth.sheet, prof, d.sheet.level, now, cat);
        if changes.iter().any(|c| matches!(c, Change::ProficiencyRaised(..))) {
            changes.extend(p.growth.check_titles(cat));
        }
        if !changes.is_empty() {
            p.dirty_stats = true;
            let player = p.player;
            if changes
                .iter()
                .any(|c| matches!(c, Change::TitleEarned(_) | Change::TitleReplaced { .. }))
            {
                // A title may raise the maximums.
                refresh_player(z, ah);
            }
            announce(z, player, &changes, out);
        }
    }
    if killed {
        on_kill(z, att, tgt, out);
    }
    damage.is_some()
}

/// Everyone in the zone counts the clear; the binary records it.
fn cleared(z: &mut Zone, d: &defs::DungeonDef, out: &mut TickOutput) {
    let content = z.content.clone();
    let cat = &content.catalog;
    let secs = z.tick.0 / base::TICKS_PER_SECOND;
    let players: Vec<(PlayerId, hecs::Entity)> = z.by_player.iter().map(|(p, h)| (*p, *h)).collect();
    for &(player, h) in &players {
        let changes = match z.world.query_one_mut::<&mut PlayerComp>(h) {
            Ok(p) => {
                p.growth.count(cat, "clears", &d.id, 1);
                p.dirty_stats = true;
                p.growth.check_titles(cat)
            }
            Err(_) => continue,
        };
        let args = smallvec::smallvec![
            Arg::Ref(Box::new(TextRef::new(d.name))),
            Arg::Int(i64::try_from(secs).unwrap_or(i64::MAX))
        ];
        z.tell(out, player, "system.dungeon_cleared", args);
        if !changes.is_empty() {
            refresh_player(z, h);
            announce(z, player, &changes, out);
        }
    }
    out.clears.push(crate::types::ClearOut {
        dungeon: d.id.clone(),
        players: players.into_iter().map(|(p, _)| p).collect(),
        secs,
    });
}

/// Turns progression changes into System messages.
pub(crate) fn announce(z: &Zone, player: PlayerId, changes: &[Change], out: &mut TickOutput) {
    let cat = &z.content.catalog;
    let int = |v: u64| Arg::Int(i64::try_from(v).unwrap_or(i64::MAX));
    let name =
        |id: Option<base::TextId>| Arg::Ref(Box::new(TextRef::new(id.unwrap_or(base::TextId::new(0)))));
    for c in changes {
        if let Change::TitleEarned(t) | Change::TitleReplaced { new: t, .. } = c
            && cat.title(*t).is_some_and(|d| !d.announce.is_quiet())
        {
            out.feats.push((player, *t));
        }
        match c {
            Change::TitleEarned(t) | Change::TitleReplaced { new: t, .. } => {
                if let Some(d) = cat.title(*t).filter(|d| d.unique.is_some()) {
                    out.claims.push((player, format!("title:{}", d.id)));
                }
            }
            Change::ClassChosen(k) => {
                if let Some(d) = cat.class(*k).filter(|d| d.unique.is_some()) {
                    out.claims.push((player, format!("class:{}", d.id)));
                }
            }
            _ => {}
        }
        let (key, args): (&str, SmallVec<[Arg; 2]>) = match c {
            Change::LeveledUp(l) => ("system.level_up", smallvec::smallvec![int(u64::from(*l))]),
            Change::LevelLost(l) => ("system.level_lost", smallvec::smallvec![int(u64::from(*l))]),
            Change::PointsGained(n) => ("system.points", smallvec::smallvec![int(u64::from(*n))]),
            Change::ProficiencyRaised(p, v) => (
                "system.proficiency_up",
                smallvec::smallvec![name(cat.proficiency(*p).map(|d| d.name)), int(u64::from(*v))],
            ),
            Change::TechniqueUnlocked(t) => (
                "system.technique_unlocked",
                smallvec::smallvec![name(cat.technique(*t).map(|d| d.name))],
            ),
            Change::TechniqueRaised(t, l) => (
                "system.technique_up",
                smallvec::smallvec![name(cat.technique(*t).map(|d| d.name)), int(u64::from(*l))],
            ),
            Change::TitleEarned(t) => (
                "system.title",
                smallvec::smallvec![name(cat.title(*t).map(|d| d.name))],
            ),
            Change::ClassesAvailable(list) => (
                "system.classes_available",
                smallvec::smallvec![int(list.len() as u64)],
            ),
            Change::ClassChosen(c) => (
                "system.class_chosen",
                smallvec::smallvec![name(cat.class(*c).map(|d| d.name))],
            ),
            Change::TitleReplaced { old, new } => (
                "system.title_replaced",
                smallvec::smallvec![
                    name(cat.title(*old).map(|d| d.name)),
                    name(cat.title(*new).map(|d| d.name))
                ],
            ),
        };
        z.tell(out, player, key, args);
    }
}

/// XP, counters and titles for a player.
fn reward(z: &mut Zone, h: hecs::Entity, xp: u64, monster: &str, out: &mut TickOutput) {
    let content = z.content.clone();
    let cat = &content.catalog;
    let Ok((p, f)) = z.world.query_one_mut::<(&mut PlayerComp, &Fighter)>(h) else {
        return;
    };
    p.growth.count(cat, "kills", monster, 1);
    // A quest may ask for this kill.
    if let Some((mid, _)) = cat.monster_by_name(monster)
        && p.data.deed(cat, crate::player::Deed::Killed(mid))
    {
        p.dirty_quests = true;
    }
    let xp = TitleBonuses::raise(xp, progression::title_bonuses(&p.growth.sheet, cat).xp_percent);
    let mut changes = progression::add_xp(&mut p.growth.sheet, xp, cat);
    changes.extend(p.growth.check_titles(cat));
    p.dirty_stats = true;
    let _ = f;
    let player = p.player;
    if changes
        .iter()
        .any(|c| matches!(c, Change::TitleEarned(_) | Change::TitleReplaced { .. }))
    {
        refresh_player(z, h);
    }
    if changes.iter().any(|c| matches!(c, Change::LeveledUp(_))) {
        // Leveling raises the maximums and heals.
        refresh_player(z, h);
        if let Ok(f) = z.world.query_one_mut::<&mut Fighter>(h) {
            f.hp = f.max_hp;
            f.vigor = f.max_vigor;
            f.mp = f.max_mp;
        }
    }
    z.tell(
        out,
        player,
        "system.xp",
        smallvec::smallvec![Arg::Int(i64::try_from(xp).unwrap_or(i64::MAX))],
    );
    announce(z, player, &changes, out);
}

/// Smallest share of a monster's threat that earns XP, in thousandths.
const MIN_CONTRIBUTION_PERMILLE: u32 = 100;
/// Bonus per extra group member who shares a kill, in thousandths.
const PARTY_BONUS_PERMILLE: u64 = 100;

/// XP of each player who earned some from a kill, sorted by entity id.
pub(crate) fn kill_xp(z: &Zone, m: &MonsterComp, monster: hecs::Entity, xp: u64) -> Vec<(EntityId, u64)> {
    let at = z.world.get::<&Body>(monster).ok().map(|b| b.pos);
    // Players who fought it: (entity, player, share).
    let mut who: Vec<(EntityId, base::PlayerId, u32)> = Vec::new();
    for id in m.threat.keys() {
        let Some(&h) = z.by_id.get(&id) else { continue };
        let Ok(p) = z.world.get::<&PlayerComp>(h) else {
            continue;
        };
        who.push((id, p.player, m.threat.share_permille(id)));
    }
    let mut out: Vec<(EntityId, u64)> = Vec::new();
    let mut done: Vec<base::PlayerId> = Vec::new();
    for &(id, player, share) in &who {
        if done.contains(&player) {
            continue;
        }
        let Some(members) = z.party_members(player) else {
            if share >= MIN_CONTRIBUTION_PERMILLE {
                out.push((id, combat::permille(xp, share)));
            }
            continue;
        };
        done.extend(members.iter().copied());
        // The group's pool: every member's share counts, but only members
        // who earned it and are near the kill take part in the split.
        let pooled: u64 = who
            .iter()
            .filter(|(_, p, _)| members.contains(p))
            .map(|(_, _, s)| u64::from(*s))
            .sum();
        let takers: Vec<EntityId> = who
            .iter()
            .filter(|(eid, p, s)| {
                members.contains(p)
                    && *s >= MIN_CONTRIBUTION_PERMILLE
                    && z.by_id
                        .get(eid)
                        .and_then(|h| z.world.get::<&Body>(*h).ok())
                        .zip(at)
                        .is_some_and(|(b, at)| b.pos.distance(at) <= crate::social::SHARE_RADIUS)
            })
            .map(|(eid, _, _)| *eid)
            .collect();
        let n = takers.len() as u64;
        if n == 0 {
            continue;
        }
        let bonus = PARTY_BONUS_PERMILLE
            .saturating_mul(n.saturating_sub(1))
            .saturating_add(1000);
        let pool = xp.saturating_mul(pooled) / 1000;
        let pool = pool.saturating_mul(bonus) / 1000;
        let each = pool.checked_div(n).unwrap_or(0);
        out.extend(takers.into_iter().map(|t| (t, each)));
    }
    out.sort_by_key(|(id, _)| *id);
    out
}

pub(crate) fn on_kill(z: &mut Zone, att: EntityId, tgt: EntityId, out: &mut TickOutput) {
    let Some(&th) = z.by_id.get(&tgt) else { return };
    let monster = z
        .world
        .get::<&MonsterComp>(th)
        .ok()
        .map(|m| MonsterComp::clone(&m));
    match monster {
        Some(m) => {
            let content = z.content.clone();
            let Some(def) = content.catalog.monster(m.monster) else {
                return;
            };
            // Whoever held at least a tenth of the threat shares the XP by
            // that share; a group pools its members' shares and splits them
            // among those near the kill, with a bonus per extra member
            // The top
            // attacker gets the loot.
            for (id, xp) in kill_xp(z, &m, th, def.xp) {
                if let Some(&h) = z.by_id.get(&id) {
                    reward(z, h, xp, &def.id, out);
                }
            }
            let looter = if z
                .by_id
                .get(&att)
                .is_some_and(|h| z.world.get::<&PlayerComp>(*h).is_ok())
            {
                Some(att)
            } else {
                m.threat.top()
            };
            if let Some(l) = looter.and_then(|l| z.by_id.get(&l).copied()) {
                loot(z, l, m.monster, out);
            }
            let respawn_at = z.tick.0.saturating_add(u64::from(def.respawn_ticks));
            z.respawns.push((respawn_at, m.spawn));
            z.remove_entity(tgt);
            // A dungeon's boss: the run is cleared.
            if let Some(d) = content
                .catalog
                .dungeon_of_zone(z.zone_id())
                .filter(|d| d.boss == Some(m.monster))
            {
                cleared(z, d, out);
            }
        }
        None => fall(z, th, out),
    }
}

fn loot(z: &mut Zone, h: hecs::Entity, monster: MonsterId, out: &mut TickOutput) {
    let content = z.content.clone();
    let cat = &content.catalog;
    let Some(def) = cat.monster(monster) else { return };
    let mut coins = u64::from(z.rng.range(def.coins.0, def.coins.1));
    let mut got: Vec<(base::ItemId, u32)> = Vec::new();
    for l in &def.loot {
        if z.rng.chance(u32::from(l.chance)) {
            got.push((l.item, z.rng.range(u32::from(l.qty.0), u32::from(l.qty.1))));
        }
    }
    // The monster's own drops, then the table it shares with others
    // (phase 7).
    if let Some(table) = def.loot_table.and_then(|t| cat.loot_table(t)) {
        let (more_coins, more) = z.roll_table(table);
        coins = coins.saturating_add(more_coins);
        got.extend(more);
    }
    // A relic that exists somewhere does not drop.
    got.retain(|(item, _)| !(cat.item(*item).is_some_and(|d| d.unique) && z.taken_items.contains(item)));
    give_loot(z, h, coins, &got, crate::types::CoinFlow::MonsterLoot, out);
}

/// Hands coins and items to a player, with the System lines for each.
pub(crate) fn give_loot(
    z: &mut Zone,
    h: hecs::Entity,
    coins: u64,
    items: &[(base::ItemId, u32)],
    flow: crate::types::CoinFlow,
    out: &mut TickOutput,
) {
    let content = z.content.clone();
    let cat = &content.catalog;
    let Some(player) = z.world.get::<&PlayerComp>(h).ok().map(|p| p.player) else {
        return;
    };
    if coins > 0 {
        if let Ok(p) = z.world.query_one_mut::<&mut PlayerComp>(h) {
            p.data.gain_coins(flow, coins);
            p.dirty_bag = true;
        }
        z.tell(
            out,
            player,
            "loot.coins",
            smallvec::smallvec![Arg::Int(i64::try_from(coins).unwrap_or(i64::MAX))],
        );
    }
    for &(item, qty) in items {
        let Some(idef) = cat.item(item) else { continue };
        let name = Arg::Ref(Box::new(TextRef::new(idef.name)));
        let ok = if idef.equip.is_some() {
            give_instance(z, h, item, out)
        } else {
            match z.world.query_one_mut::<&mut PlayerComp>(h) {
                Ok(p) => {
                    let ok = p.data.give(cat, item, qty).is_ok();
                    p.dirty_bag |= ok;
                    ok
                }
                Err(_) => false,
            }
        };
        let key = if ok { "loot.item" } else { "loot.full" };
        z.tell(
            out,
            player,
            key,
            smallvec::smallvec![name, Arg::Int(i64::from(qty))],
        );
    }
}

/// Wears one equipped piece of a player, warns at the thresholds of
/// `economy/formulas.ron` and destroys it at zero
fn wear(z: &mut Zone, h: hecs::Entity, slot: EquipSlot, out: &mut TickOutput) {
    let loss = z.content.catalog.formulas().economy.durability_per_hit;
    wear_by(z, h, slot, loss, out);
}

/// Once a minute, every equipped piece that burns (a torch) loses its
/// `burn` in durability; at zero it is gone.
pub(crate) fn burn(z: &mut Zone, out: &mut TickOutput) {
    const MINUTE: u64 = 60 * base::TICKS_PER_SECOND;
    if !z.tick.0.is_multiple_of(MINUTE) {
        return;
    }
    let content = z.content.clone();
    let cat = &content.catalog;
    let mut burning = Vec::new();
    for (h, (gear, p)) in z.world.query::<(&Gear, &PlayerComp)>().iter() {
        for (slot, inst) in &gear.equipped {
            if let Some(b) = cat.item(inst.item).map(|d| d.burn).filter(|b| *b > 0) {
                burning.push((p.player, *slot, h, b));
            }
        }
    }
    // World order is not part of the state: sort for the replay.
    burning.sort_by_key(|(p, s, _, _)| (*p, *s));
    for (_, slot, h, b) in burning {
        wear_by(z, h, slot, b, out);
    }
}

fn wear_by(z: &mut Zone, h: hecs::Entity, slot: EquipSlot, loss: u32, out: &mut TickOutput) {
    let content = z.content.clone();
    let cat = &content.catalog;
    let economy = &cat.formulas().economy;
    let (first, second) = economy.warn_durability_permille;
    if loss == 0 || z.world.get::<&PlayerComp>(h).is_err() {
        return;
    }
    let Some(player) = z.world.get::<&PlayerComp>(h).ok().map(|p| p.player) else {
        return;
    };
    let mut broke = None;
    let mut warn = None;
    let mut name = None;
    if let Ok(gear) = z.world.query_one_mut::<&mut Gear>(h) {
        let Some(inst) = gear.equipped.iter_mut().find(|(s, _)| *s == slot).map(|(_, i)| i) else {
            return;
        };
        if inst.max_durability == 0 {
            return;
        }
        let before = inst.durability;
        name = Some(inst.item);
        if inst.wear(loss) {
            broke = Some(());
        } else if inst.crossed(before, second) {
            warn = Some(second);
        } else if inst.crossed(before, first) {
            warn = Some(first);
        }
    }
    let label = |z: &Zone, item: Option<base::ItemId>| {
        item.and_then(|i| z.content.catalog.item(i).map(|d| d.name))
            .map(|n| Arg::Ref(Box::new(TextRef::new(n))))
            .unwrap_or(Arg::Int(0))
    };
    if broke.is_some() {
        if let Ok(gear) = z.world.query_one_mut::<&mut Gear>(h) {
            let _ = gear.take_equipped(slot);
        }
        if let Some(item) = name {
            released(z, item, out);
        }
        refresh_player(z, h);
        if let Ok(p) = z.world.query_one_mut::<&mut PlayerComp>(h) {
            p.dirty_bag = true;
            p.dirty_stats = true;
        }
        let args: SmallVec<[Arg; 2]> = smallvec::smallvec![label(z, name)];
        z.tell(out, player, "system.item_broken", args);
        return;
    }
    if let Some(permille) = warn {
        if let Ok(p) = z.world.query_one_mut::<&mut PlayerComp>(h) {
            p.dirty_bag = true;
        }
        let args: SmallVec<[Arg; 2]> =
            smallvec::smallvec![label(z, name), Arg::Int(i64::from(permille / 10))];
        z.tell(out, player, "system.durability_low", args);
    }
}

/// Death costs a share of the durability of everything equipped.
pub(crate) fn wear_on_death(z: &mut Zone, h: hecs::Entity) {
    let permille = z.content.catalog.formulas().economy.death_durability_permille;
    if permille == 0 {
        return;
    }
    if let Ok(gear) = z.world.query_one_mut::<&mut Gear>(h) {
        for (_, inst) in gear.equipped.iter_mut() {
            let loss = u32::try_from(combat::permille(u64::from(inst.max_durability), permille))
                .unwrap_or(0)
                .max(1);
            // Death never breaks a piece outright: it leaves one blow.
            inst.durability = inst.durability.saturating_sub(loss).max(1);
        }
    }
}

/// A new item with identity into the first free backpack slot. A unique
/// item that exists already is not made (false).
pub(crate) fn give_instance(z: &mut Zone, h: hecs::Entity, item: base::ItemId, out: &mut TickOutput) -> bool {
    let content = z.content.clone();
    let unique = content.catalog.item(item).is_some_and(|d| d.unique);
    if unique && z.taken_items.contains(&item) {
        return false;
    }
    let Some(ids) = z.item_ids.as_mut() else {
        out.need_item_ids = true;
        return false;
    };
    let Ok((p, gear)) = z.world.query_one_mut::<(&mut PlayerComp, &mut Gear)>(h) else {
        return false;
    };
    let Some(slot) = p.data.slots.iter().position(|s| *s == Slot::Empty) else {
        return false;
    };
    let Some(inst) = ItemInstance::create(item, &content.catalog, ids, &mut z.rng) else {
        out.need_item_ids = true;
        return false;
    };
    if ids.remaining() < LOW_ITEM_IDS {
        out.need_item_ids = true;
    }
    if let Some(s) = p.data.slots.get_mut(slot) {
        *s = Slot::Held(item);
    }
    gear.bag.push((slot as u16, inst));
    p.dirty_bag = true;
    if unique {
        let player = p.player;
        z.taken_items.insert(item);
        if let Some(d) = content.catalog.item(item) {
            out.claims.push((player, format!("item:{}", d.id)));
        }
    }
    true
}

/// Takes one piece of `item` out of a player's backpack or hands (a unique
/// piece someone else made first). True when there was one.
pub(crate) fn remove_piece(z: &mut Zone, h: hecs::Entity, item: base::ItemId) -> bool {
    let Ok((p, gear)) = z.world.query_one_mut::<(&mut PlayerComp, &mut Gear)>(h) else {
        return false;
    };
    if let Some(slot) = gear.bag.iter().find(|(_, i)| i.item == item).map(|(s, _)| *s) {
        let _ = gear.take_bag(slot);
        if let Some(s) = p.data.slots.get_mut(usize::from(slot)) {
            *s = Slot::Empty;
        }
        p.dirty_bag = true;
        return true;
    }
    let worn = gear
        .equipped
        .iter()
        .find(|(_, i)| i.item == item)
        .map(|(s, _)| *s);
    if let Some(slot) = worn {
        let _ = gear.take_equipped(slot);
        p.dirty_bag = true;
        p.dirty_stats = true;
        return true;
    }
    false
}

/// A unique piece left the game (broken, sold): it may exist again.
pub(crate) fn released(z: &mut Zone, item: base::ItemId, out: &mut TickOutput) {
    if let Some(d) = z.content.catalog.item(item).filter(|d| d.unique) {
        z.taken_items.remove(&item);
        out.released.push(format!("item:{}", d.id));
    }
}

/// HP at zero: fallen, waiting for help.
fn fall(z: &mut Zone, h: hecs::Entity, out: &mut TickOutput) {
    let ticks = u64::from(z.content.catalog.formulas().progression.fallen_ticks);
    let until = z.tick.0.saturating_add(ticks.max(1));
    let Ok((f, walk, p)) = z.world.query_one_mut::<(&mut Fighter, &mut Walk, &PlayerComp)>(h) else {
        return;
    };
    f.hp = 0;
    f.fallen_until = until;
    f.target = None;
    walk.steps.clear();
    let player = p.player;
    z.tell(
        out,
        player,
        "system.fallen",
        smallvec::smallvec![Arg::Int(i64::try_from(ticks / 20).unwrap_or(0))],
    );
}

/// The wait is over (or the player gave up): death.
pub(crate) fn die(z: &mut Zone, h: hecs::Entity, out: &mut TickOutput) {
    let content = z.content.clone();
    let cat = &content.catalog;
    let f = &cat.formulas().progression;
    let spawn = z.spawn_point(EntityId::new(0));
    let spot = z.nearest_free(spawn);
    let now_unix = z.now_unix;
    let Ok((fi, p, body)) = z
        .world
        .query_one_mut::<(&mut Fighter, &mut PlayerComp, &mut Body)>(h)
    else {
        return;
    };
    p.growth.count(cat, "deaths", "", 1);
    p.dirty_stats = true;
    let player = p.player;
    if p.growth.sheet.level <= f.newbie_level {
        // Newbie protection: no loss, back to the sanctuary (zone spawn).
        fi.fallen_until = 0;
        fi.hp = fi.max_hp;
        fi.vigor = fi.max_vigor;
        fi.mp = fi.max_mp;
        let old = body.pos;
        body.pos = spot;
        body.moved_tick = z.tick.0;
        body.status_tick = z.tick.0;
        z.grid.remove(body.id, old);
        z.grid.insert(body.id, spot);
        z.tell(out, player, "system.revived", SmallVec::new());
        // In a dungeon copy the sanctuary is outside: back to its exit.
        if let Some(rule) = cat.zone(z.zone_id()).and_then(|d| d.instance) {
            crate::portal::teleport(z, player, rule.exit, out);
        }
        return;
    }
    let changes = progression::lose_level(&mut p.growth.sheet);
    p.locked_until = now_unix.saturating_add(f.death_lock_secs);
    fi.fallen_until = u64::MAX;
    fi.hp = fi.max_hp;
    // Hard death also costs durability.
    wear_on_death(z, h);
    announce(z, player, &changes, out);
    z.tell(
        out,
        player,
        "system.died",
        smallvec::smallvec![Arg::Int(i64::try_from(f.death_lock_secs / 3600).unwrap_or(0))],
    );
    out.dead.push(player);
}

/// Monsters: pick a target, chase it, hit it, give up past the leash.
pub(crate) fn ai(z: &mut Zone, out: &mut TickOutput) {
    let now = z.tick.0;
    let content = z.content.clone();
    let cat = &content.catalog;
    z.with_entities_of::<MonsterComp, _>(|z, monsters| {
        for &(id, h) in monsters {
            let Some((pos, m, fighter)) = (|| {
                let b = z.world.get::<&Body>(h).ok()?.pos;
                let m = MonsterComp::clone(&*z.world.get::<&MonsterComp>(h).ok()?);
                let f = Fighter::clone(&*z.world.get::<&Fighter>(h).ok()?);
                Some((b, m, f))
            })() else {
                continue;
            };
            let Some(def) = cat.monster(m.monster) else {
                continue;
            };
            // Stunned: no step, no blow.
            if fighter.stunned(now) {
                continue;
            }
            let valid = |z: &Zone, t: EntityId| -> bool {
                z.body_of(t).is_some_and(|(th, tpos)| {
                    z.world.get::<&Fighter>(th).is_ok_and(|f| f.alive())
                        && z.world.get::<&PlayerComp>(th).is_ok()
                        && z.region(tpos) != Some(RegionKind::Safe)
                        && tpos.distance(m.home) <= u32::from(def.leash)
                })
            };
            // Past the leash: forget everything and walk home healed.
            if pos.distance(m.home) > u32::from(def.leash) {
                reset_monster(z, h, pos, m.home);
                continue;
            }
            let mut target = fighter.target.filter(|t| valid(z, *t));
            if target.is_none() {
                target = m.threat.keys().filter(|t| valid(z, *t)).min_by_key(|t| t.get());
                if let Some(top) = m.threat.top().filter(|t| valid(z, *t)) {
                    target = Some(top);
                }
            }
            if target.is_none() {
                // Aggro: nearest visible player inside the radius.
                let mut near = Vec::new();
                z.grid.neighbors(pos, def.aggro_radius, &mut near);
                near.sort_by_key(|(e, p)| (pos.distance(*p), e.get()));
                target = near
                    .iter()
                    .find(|(e, p)| *e != id && valid(z, *e) && world::line_of_sight(&z.map, pos, *p))
                    .map(|(e, _)| *e);
            }
            let Some(t) = target else {
                if let Ok((f, mc)) = z.world.query_one_mut::<(&mut Fighter, &mut MonsterComp)>(h) {
                    f.target = None;
                    mc.threat.clear();
                }
                if pos != m.home {
                    walk_to(z, h, pos, m.home, now);
                }
                continue;
            };
            if let Ok(f) = z.world.query_one_mut::<&mut Fighter>(h) {
                f.target = Some(t);
            }
            let tpos = z.body_of(t).map(|(_, p)| p);
            // A caster steps back from a target that came next to it.
            if def.range > 1
                && let Some(tpos) = tpos
                && pos.distance(tpos) <= 1
                && let Some(away) = step_away(z, pos, tpos, m.home, def.leash)
            {
                walk_to(z, h, pos, away, now);
                continue;
            }
            match z.can_hit(id, t, def.range) {
                Ok(()) => {
                    // In reach it stands, so a caster does not walk on in.
                    if let Ok(w) = z.world.query_one_mut::<&mut Walk>(h) {
                        w.steps.clear();
                    }
                    if now >= fighter.next_attack {
                        // A boss hits as its phase says.
                        let phase = crate::status::update_phase(z, h, &def.phases, def.name, out);
                        let ticks = phase.and_then(|p| p.attack_ticks).unwrap_or(def.attack_ticks);
                        if let Ok(f) = z.world.query_one_mut::<&mut Fighter>(h) {
                            f.next_attack = now.saturating_add(f.paced(u64::from(ticks), now));
                        }
                        let (mult, area, status) = phase.map_or((1000, 0, None), |p| {
                            (p.damage_permille, p.area, p.status.as_ref())
                        });
                        crate::status::blow(
                            z,
                            id,
                            t,
                            &crate::status::Hit {
                                mult,
                                technique: None,
                                area,
                                status,
                            },
                            out,
                        );
                    }
                }
                Err(_) => {
                    if let Some((_, tpos)) = z.body_of(t) {
                        walk_to(z, h, pos, tpos, now);
                    }
                }
            }
        }
    });
}

/// A free neighbor of `pos` farther from `from`, inside the leash; the
/// farthest one, first in a fixed order on ties.
fn step_away(z: &Zone, pos: TilePos, from: TilePos, home: TilePos, leash: u16) -> Option<TilePos> {
    let mut best: Option<(u32, TilePos)> = None;
    for (dx, dy) in [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ] {
        let p = TilePos::new(pos.x.saturating_add(dx), pos.y.saturating_add(dy));
        let d = p.distance(from);
        if d > pos.distance(from)
            && p.distance(home) <= u32::from(leash)
            && z.free(p)
            && best.is_none_or(|(b, _)| d > b)
        {
            best = Some((d, p));
        }
    }
    best.map(|(_, p)| p)
}

fn reset_monster(z: &mut Zone, h: hecs::Entity, pos: TilePos, home: TilePos) {
    let now = z.tick.0;
    if let Ok((f, m, b)) = z
        .world
        .query_one_mut::<(&mut Fighter, &mut MonsterComp, &mut Body)>(h)
    {
        f.target = None;
        f.hp = f.max_hp;
        f.last_path = 0;
        f.statuses.clear();
        m.threat.clear();
        m.phase = 0;
        b.status_tick = now;
    }
    walk_to(z, h, pos, home, now);
}

/// Path toward `to`, at most every `REPATH_TICKS`.
fn walk_to(z: &mut Zone, h: hecs::Entity, from: TilePos, to: TilePos, now: u64) {
    let Ok((f, w)) = z.world.query_one_mut::<(&mut Fighter, &mut Walk)>(h) else {
        return;
    };
    if !w.steps.is_empty() && now < f.last_path.saturating_add(REPATH_TICKS) {
        return;
    }
    f.last_path = now;
    let grid = &z.grid;
    w.steps = world::path_avoiding(&z.map, from, to, crate::zone::PATH_MAX_NODES, |t| {
        crate::movement::occupied_near(grid, from, to, t)
    })
    .map(VecDeque::from)
    .unwrap_or_default();
}

/// Players' weapons hit their target on their own interval; out of reach
/// they walk to it.
pub(crate) fn attacks(z: &mut Zone, out: &mut TickOutput) {
    let now = z.tick.0;
    z.with_player_handles(|z, players| {
        for &h in players {
            let Some((id, pos, fighter)) = (|| {
                let b = z.world.get::<&Body>(h).ok()?;
                let f = Fighter::clone(&*z.world.get::<&Fighter>(h).ok()?);
                Some((b.id, b.pos, f))
            })() else {
                continue;
            };
            let Some(t) = fighter.target else { continue };
            if !fighter.alive() || fighter.stunned(now) {
                continue;
            }
            let Some(a) = arms(z, h) else { continue };
            match z.can_hit(id, t, a.range) {
                Ok(()) => {
                    if now >= fighter.next_attack {
                        if let Ok(f) = z.world.query_one_mut::<&mut Fighter>(h) {
                            let wait = blow_wait(f, a.attack_ticks, a.vigor);
                            f.next_attack = now.saturating_add(f.paced(wait, now));
                        }
                        strike(z, id, t, 1000, None, out);
                    }
                }
                // Retaliation never chases: out of reach, it ends.
                Err(Refusal::TooFar) if fighter.retaliating => {
                    if let Ok(f) = z.world.query_one_mut::<&mut Fighter>(h) {
                        f.target = None;
                        f.retaliating = false;
                    }
                }
                Err(Refusal::TooFar) => {
                    let walking = z.world.get::<&Walk>(h).is_ok_and(|w| !w.steps.is_empty());
                    if !walking && let Some((_, tpos)) = z.body_of(t) {
                        walk_to(z, h, pos, tpos, now);
                    }
                }
                Err(_) => {
                    if let Ok(f) = z.world.query_one_mut::<&mut Fighter>(h) {
                        f.target = None;
                    }
                }
            }
        }
    });
}

/// HP out of combat and vigor, every `regen_interval_ticks`.
pub(crate) fn regen(z: &mut Zone) {
    let now = z.tick.0;
    let f = z.content.catalog.formulas().progression.clone();
    if !now.is_multiple_of(u64::from(f.regen_interval_ticks)) {
        return;
    }
    for (_, (fi, body, p)) in z
        .world
        .query_mut::<(&mut Fighter, &mut Body, Option<&mut PlayerComp>)>()
    {
        if fi.hp == 0 || fi.fallen_until != 0 {
            continue;
        }
        let before = (fi.hp, fi.vigor, fi.mp);
        // A vow that ran out: the status window shows the plain defense.
        let ward_ended = fi.ward.until != 0 && now >= fi.ward.until;
        if ward_ended {
            fi.ward = Ward::default();
        }
        if now.saturating_sub(fi.last_combat) >= u64::from(f.out_of_combat_ticks) && fi.hp < fi.max_hp {
            let add = u32::try_from(combat::permille(u64::from(fi.max_hp), f.hp_regen_permille).max(1))
                .unwrap_or(u32::MAX);
            fi.hp = fi.hp.saturating_add(add).min(fi.max_hp);
        }
        if fi.vigor < fi.max_vigor {
            let add = u32::try_from(combat::permille(u64::from(fi.max_vigor), f.vigor_regen_permille).max(1))
                .unwrap_or(u32::MAX);
            fi.vigor = fi.vigor.saturating_add(add).min(fi.max_vigor);
        }
        if fi.mp < fi.max_mp {
            fi.mp = fi.mp.saturating_add(fi.mp_regen.max(1)).min(fi.max_mp);
        }
        if before.0 != fi.hp {
            body.status_tick = now;
        }
        if (ward_ended || before != (fi.hp, fi.vigor, fi.mp))
            && let Some(p) = p
        {
            p.dirty_stats = true;
        }
    }
}

/// Fallen players whose wait ended die.
pub(crate) fn fallen(z: &mut Zone, out: &mut TickOutput) {
    let now = z.tick.0;
    // `die` only ever touches its own entity, so the wait is checked as
    // each player comes up instead of listing them first.
    z.with_player_handles(|z, players| {
        for &h in players {
            let due = z
                .world
                .get::<&Fighter>(h)
                .is_ok_and(|f| f.fallen_until != 0 && f.fallen_until != u64::MAX && now >= f.fallen_until);
            if due {
                die(z, h, out);
            }
        }
    });
}

/// Monsters whose respawn time came.
pub(crate) fn respawn(z: &mut Zone) {
    let now = z.tick.0;
    let (due, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut z.respawns)
        .into_iter()
        .partition(|(t, _)| *t <= now);
    z.respawns = wait;
    for (_, spawn) in due {
        z.spawn_monster(spawn);
    }
}

impl Zone {
    /// An offset in `-r..=r` from the zone's generator.
    fn wander(&mut self, r: i32) -> i32 {
        let span = u32::try_from(r.saturating_mul(2)).unwrap_or(0);
        i32::try_from(self.rng.range(0, span))
            .unwrap_or(0)
            .saturating_sub(r)
    }

    /// Places one monster of entry `spawn` of `monsters.ron` on a free
    /// tile near its point (deterministic: the zone's generator).
    pub(crate) fn spawn_monster(&mut self, spawn: u16) {
        let content = self.content.clone();
        let cat = &content.catalog;
        let Some(s) = self.monster_spawns.get(usize::from(spawn)).cloned() else {
            return;
        };
        let Some((mid, def)) = cat.monster_by_name(&s.monster) else {
            return;
        };
        let r = i32::from(s.radius);
        let mut pos = s.pos;
        for _ in 0..8 {
            let p = TilePos::new(
                s.pos.x.saturating_add(self.wander(r)),
                s.pos.y.saturating_add(self.wander(r)),
            );
            if self.free(p) && self.region(p) != Some(RegionKind::Safe) {
                pos = p;
                break;
            }
        }
        let pos = self.nearest_free(pos);
        if !self.free(pos) {
            self.respawns.push((self.tick.0.saturating_add(20), spawn));
            return;
        }
        let id = EntityId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.insert_entity(
            id,
            pos,
            (
                Body {
                    id,
                    pos,
                    facing: base::Direction::S,
                    moved_tick: 0,
                    status_tick: 0,
                },
                Walk::default(),
                Fighter::new(def.hp, 0, def.level),
                MonsterComp {
                    monster: mid,
                    spawn,
                    home: pos,
                    threat: ThreatTable::default(),
                    phase: 0,
                },
            ),
        );
    }
}

/// HP and vigor maximums of a player from sheet and gear.
pub(crate) fn player_fighter(z: &Zone, g: &Growth, gear: &Gear, hp: u32) -> Fighter {
    let cat = &z.content.catalog;
    let attrs = g.attributes(&gear.bonus(), cat);
    let titles = progression::title_bonuses(&g.sheet, cat);
    let max_hp = cat
        .formulas()
        .max_hp(&attrs, g.sheet.level)
        .saturating_add(titles.max_hp);
    let max_vigor = cat.formulas().max_vigor(&attrs).saturating_add(titles.max_vigor);
    let mut f = Fighter::new(max_hp, max_vigor, g.sheet.level);
    f.max_mp = raise_mp(
        cat.formulas().max_mp(&attrs, g.sheet.level),
        titles.max_mp_percent,
    );
    f.mp = f.max_mp;
    f.mp_regen = cat.formulas().mp_regen(&attrs, f.max_mp);
    if hp > 0 {
        f.hp = hp.min(max_hp);
    }
    f
}

/// Maximum MP with the titles' percent (P03).
fn raise_mp(base: u32, percent: u32) -> u32 {
    u32::try_from(progression::TitleBonuses::raise(u64::from(base), percent)).unwrap_or(u32::MAX)
}

/// Recomputes a player's maximums after points, class or equipment change.
pub(crate) fn refresh_player(z: &mut Zone, h: hecs::Entity) {
    let content = z.content.clone();
    let cat = &content.catalog;
    let now = z.tick.0;
    let Ok((f, p, gear, body)) = z
        .world
        .query_one_mut::<(&mut Fighter, &PlayerComp, &Gear, &mut Body)>(h)
    else {
        return;
    };
    let attrs = p.growth.attributes(&gear.bonus(), cat);
    let titles = progression::title_bonuses(&p.growth.sheet, cat);
    f.level = p.growth.sheet.level;
    f.max_hp = cat
        .formulas()
        .max_hp(&attrs, f.level)
        .saturating_add(titles.max_hp);
    f.max_vigor = cat.formulas().max_vigor(&attrs).saturating_add(titles.max_vigor);
    f.hp = f.hp.min(f.max_hp);
    f.vigor = f.vigor.min(f.max_vigor);
    f.max_mp = raise_mp(cat.formulas().max_mp(&attrs, f.level), titles.max_mp_percent);
    f.mp_regen = cat.formulas().mp_regen(&attrs, f.max_mp);
    f.mp = f.mp.min(f.max_mp);
    body.status_tick = now;
}
