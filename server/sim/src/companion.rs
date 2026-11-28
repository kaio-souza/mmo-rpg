//! Companions: monsters a player tamed. Taming follows the monster's own
//! `tame` block (requirements the designer chose, weakened below a share
//! of HP, a chance). A companion keeps the monster's stats and art, follows
//! its owner, helps against the owner's target, takes threat like anyone
//! and, when it falls, rests before it can be called again.
//!
//! The list lives in the owner's data (saved by monster id); the creature
//! that follows is an entity of the zone, made again wherever the owner
//! enters, so it travels with every hand-over and instance.

use crate::Zone;
use crate::fight::{Fighter, MonsterComp};
use crate::types::{CompanionView, Refusal, SavedCompanion, TickOutput};
use crate::zone::{Body, PlayerComp, Walk};
use base::{Arg, EntityId, MonsterId, PlayerId, TextRef, TilePos};
use defs::TameReq;
use serde::{Deserialize, Serialize};

/// A companion is pulled to its owner's side past this distance.
const CALL_BACK_DISTANCE: u32 = 12;
/// It helps against targets this close to its owner.
const ASSIST_DISTANCE: u32 = 10;
/// Tiles from the monster a player tames from.
pub(crate) const TAME_REACH: u32 = 2;

/// The creature of a player's companion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CompanionComp {
    pub owner: PlayerId,
    pub monster: MonsterId,
    /// Position in the owner's list.
    pub index: u8,
    /// The name its owner gave it.
    #[serde(default)]
    pub name: Option<base::CharacterName>,
}

fn monster_arg(z: &Zone, monster: MonsterId) -> Arg {
    let name = z
        .content
        .catalog
        .monster(monster)
        .map_or(base::TextId::new(0), |d| d.name);
    Arg::Ref(Box::new(TextRef::new(name)))
}

impl Zone {
    /// The entity of `player`'s companion in this zone.
    pub(crate) fn companion_of(&self, player: PlayerId) -> Option<(hecs::Entity, EntityId)> {
        let mut found: Vec<(EntityId, hecs::Entity)> = self
            .world
            .query::<(&CompanionComp, &Body)>()
            .iter()
            .filter(|(_, (c, _))| c.owner == player)
            .map(|(h, (_, b))| (b.id, h))
            .collect();
        found.sort_by_key(|(id, _)| *id);
        found.first().map(|(id, h)| (*h, *id))
    }

    /// `/tame`: the player tries to tame `target`.
    pub(crate) fn tame(
        &mut self,
        player: PlayerId,
        target: EntityId,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let content = self.content.clone();
        let cat = &content.catalog;
        let &h = self.by_player.get(&player).ok_or(Refusal::InvalidTarget)?;
        let (th, tpos) = self.body_of(target).ok_or(Refusal::InvalidTarget)?;
        let m = self
            .world
            .get::<&MonsterComp>(th)
            .map(|m| MonsterComp::clone(&m))
            .map_err(|_| Refusal::InvalidTarget)?;
        let def = cat.monster(m.monster).ok_or(Refusal::InvalidTarget)?;
        let arg = monster_arg(self, m.monster);
        let Some(tame) = &def.tame else {
            self.tell(out, player, "system.tame_never", smallvec::smallvec![arg]);
            return Ok(());
        };
        let pos = self
            .world
            .get::<&Body>(h)
            .map_err(|_| Refusal::InvalidTarget)?
            .pos;
        if pos.distance(tpos) > TAME_REACH {
            return Err(Refusal::TooFar);
        }
        let (hp, max_hp) = self
            .world
            .get::<&Fighter>(th)
            .map(|f| (f.hp, f.max_hp))
            .map_err(|_| Refusal::InvalidTarget)?;
        if hp == 0 {
            return Err(Refusal::InvalidTarget);
        }
        let share = u64::from(hp)
            .saturating_mul(1000)
            .checked_div(u64::from(max_hp.max(1)))
            .unwrap_or(0);
        if share > u64::from(tame.below_hp_permille) {
            self.tell(out, player, "system.tame_too_strong", smallvec::smallvec![arg]);
            return Ok(());
        }
        let max = cat.formulas().progression.max_companions;
        {
            let p = self
                .world
                .get::<&PlayerComp>(h)
                .map_err(|_| Refusal::InvalidTarget)?;
            if p.data.companions.len() >= usize::from(max) {
                drop(p);
                self.tell(
                    out,
                    player,
                    "system.tame_full",
                    smallvec::smallvec![Arg::Int(i64::from(max))],
                );
                return Ok(());
            }
            if !meets(&p, &tame.requires) {
                drop(p);
                self.tell(out, player, "system.tame_requires", smallvec::smallvec![arg]);
                return Ok(());
            }
        }
        // The bait is taken on every attempt.
        if let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(h) {
            for r in &tame.requires {
                if let TameReq::Item { item, qty } = r {
                    let _ = p.data.take(*item, *qty);
                    p.dirty_bag = true;
                }
            }
        }
        if !self.rng.chance(u32::from(tame.chance_permille)) {
            // It fights back at whoever tried.
            let me = self.id_of(h);
            if let (Some(me), Ok(mc)) = (me, self.world.query_one_mut::<&mut MonsterComp>(th)) {
                mc.threat.add(me, 1);
            }
            self.tell(out, player, "system.tame_failed", smallvec::smallvec![arg]);
            return Ok(());
        }
        // Tamed: the wild one leaves the world (and comes back later, as if
        // it had been defeated), and a companion joins the list.
        let respawn_at = self.tick.0.saturating_add(u64::from(def.respawn_ticks));
        self.respawns.push((respawn_at, m.spawn));
        self.remove_entity(target);
        crate::server_goal::contribute(self, "tames", &def.id, 1, out);
        let (index, changes, out_now) = {
            let p = self
                .world
                .query_one_mut::<&mut PlayerComp>(h)
                .map_err(|_| Refusal::InvalidTarget)?;
            p.data.companions.push(SavedCompanion {
                monster: def.id.clone(),
                name: None,
                rest_until: 0,
            });
            p.growth.count(cat, "tames", &def.id, 1);
            p.dirty_stats = true;
            let index = u8::try_from(p.data.companions.len().saturating_sub(1)).unwrap_or(u8::MAX);
            (index, p.growth.check_titles(cat), p.data.summoned.is_some())
        };
        self.tell(out, player, "system.tamed", smallvec::smallvec![arg]);
        crate::fight::announce(self, player, &changes, out);
        if !out_now {
            let _ = self.summon(player, index, out);
        }
        Ok(())
    }

    /// Calls the companion at `index` to follow `player`.
    pub(crate) fn summon(
        &mut self,
        player: PlayerId,
        index: u8,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let &h = self.by_player.get(&player).ok_or(Refusal::InvalidTarget)?;
        let (monster, rest_until) = {
            let p = self
                .world
                .get::<&PlayerComp>(h)
                .map_err(|_| Refusal::InvalidTarget)?;
            let c = p
                .data
                .companions
                .get(usize::from(index))
                .ok_or(Refusal::InvalidSlot)?;
            (c.monster.clone(), c.rest_until)
        };
        let name = self
            .world
            .get::<&PlayerComp>(h)
            .ok()
            .and_then(|p| p.data.companions.get(usize::from(index))?.name.clone());
        if rest_until > self.now_unix {
            let secs = i64::try_from(rest_until.saturating_sub(self.now_unix)).unwrap_or(i64::MAX);
            self.tell(
                out,
                player,
                "system.companion_resting",
                smallvec::smallvec![Arg::Int(secs)],
            );
            return Ok(());
        }
        let Some((mid, _)) = self.content.catalog.monster_by_name(&monster) else {
            return Err(Refusal::InvalidSlot);
        };
        self.dismiss(player);
        if let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(h) {
            p.data.summoned = Some(index);
            p.dirty_stats = true;
        }
        self.spawn_companion(player, mid, index, name);
        Ok(())
    }

    /// Makes the creature of `player`'s companion next to them.
    fn spawn_companion(
        &mut self,
        player: PlayerId,
        monster: MonsterId,
        index: u8,
        name: Option<base::CharacterName>,
    ) {
        let Some(&h) = self.by_player.get(&player) else {
            return;
        };
        let Some(def) = self.content.catalog.monster(monster) else {
            return;
        };
        let (hp, level) = (def.hp, def.level);
        let Ok(near) = self.world.get::<&Body>(h).map(|b| b.pos) else {
            return;
        };
        let pos = self.free_near(near);
        if !self.free(pos) {
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
                Fighter::new(hp, 0, level),
                CompanionComp {
                    owner: player,
                    monster,
                    index,
                    name,
                },
            ),
        );
    }

    /// A free tile next to `pos` (in a fixed order), else the nearest free.
    fn free_near(&self, pos: TilePos) -> TilePos {
        for (dx, dy) in [
            (1, 0),
            (0, 1),
            (-1, 0),
            (0, -1),
            (1, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
        ] {
            let p = TilePos::new(pos.x.saturating_add(dx), pos.y.saturating_add(dy));
            if self.free(p) {
                return p;
            }
        }
        self.nearest_free(pos)
    }

    /// Takes the creature of `player`'s companion out of the zone; the list
    /// keeps which one was out (for `leave`) unless `forget`.
    fn despawn_companion(&mut self, player: PlayerId) {
        if let Some((_, id)) = self.companion_of(player) {
            self.remove_entity(id);
        }
    }

    /// Sends the companion that follows back.
    pub(crate) fn dismiss(&mut self, player: PlayerId) {
        self.despawn_companion(player);
        if let Some(&h) = self.by_player.get(&player)
            && let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(h)
            && p.data.summoned.is_some()
        {
            p.data.summoned = None;
            p.dirty_stats = true;
        }
    }

    /// Lets the companion at `index` go for good.
    pub(crate) fn release_companion(&mut self, player: PlayerId, index: u8) -> Result<(), Refusal> {
        let &h = self.by_player.get(&player).ok_or(Refusal::InvalidTarget)?;
        let out_now = {
            let p = self
                .world
                .get::<&PlayerComp>(h)
                .map_err(|_| Refusal::InvalidTarget)?;
            if usize::from(index) >= p.data.companions.len() {
                return Err(Refusal::InvalidSlot);
            }
            p.data.summoned
        };
        if out_now == Some(index) {
            self.dismiss(player);
        }
        let p = self
            .world
            .query_one_mut::<&mut PlayerComp>(h)
            .map_err(|_| Refusal::InvalidTarget)?;
        p.data.companions.remove(usize::from(index));
        // The one out keeps following: its position in the list moved.
        if let Some(i) = p.data.summoned
            && i > index
        {
            p.data.summoned = Some(i.saturating_sub(1));
        }
        p.dirty_stats = true;
        let summoned = p.data.summoned;
        if let (Some(i), Some((ch, _))) = (summoned, self.companion_of(player))
            && let Ok(c) = self.world.query_one_mut::<&mut CompanionComp>(ch)
        {
            c.index = i;
        }
        Ok(())
    }

    /// Gives the companion at `index` a name (`None`: its monster's again).
    pub(crate) fn name_companion(
        &mut self,
        player: PlayerId,
        index: u8,
        name: Option<base::CharacterName>,
    ) -> Result<(), Refusal> {
        let &h = self.by_player.get(&player).ok_or(Refusal::InvalidTarget)?;
        let p = self
            .world
            .query_one_mut::<&mut PlayerComp>(h)
            .map_err(|_| Refusal::InvalidTarget)?;
        let c = p
            .data
            .companions
            .get_mut(usize::from(index))
            .ok_or(Refusal::InvalidSlot)?;
        c.name = name.clone();
        p.dirty_stats = true;
        // The one out shows its new name at once.
        if let Some((ch, _)) = self.companion_of(player)
            && let Ok(comp) = self.world.query_one_mut::<&mut CompanionComp>(ch)
            && comp.index == index
        {
            comp.name = name;
        }
        Ok(())
    }

    /// On entering: the companion that was out comes along.
    pub(crate) fn companion_enters(&mut self, player: PlayerId) {
        let Some(&h) = self.by_player.get(&player) else {
            return;
        };
        let Some((index, monster, name)) = self.world.get::<&PlayerComp>(h).ok().and_then(|p| {
            let i = p.data.summoned?;
            let c = p.data.companions.get(usize::from(i))?;
            (c.rest_until <= self.now_unix).then(|| (i, c.monster.clone(), c.name.clone()))
        }) else {
            return;
        };
        if let Some((mid, _)) = self.content.catalog.monster_by_name(&monster) {
            self.spawn_companion(player, mid, index, name);
        }
    }

    /// On leaving: the creature goes with its owner (only the list is saved).
    pub(crate) fn companion_leaves(&mut self, player: PlayerId) {
        self.despawn_companion(player);
    }

    /// A companion fell: it rests, and its owner is told.
    pub(crate) fn companion_fell(&mut self, h: hecs::Entity, out: &mut TickOutput) {
        let Some((c, id)) = self
            .world
            .get::<&CompanionComp>(h)
            .ok()
            .map(|c| CompanionComp::clone(&c))
            .zip(self.id_of(h))
        else {
            return;
        };
        self.remove_entity(id);
        let rest = u64::from(self.content.catalog.formulas().progression.companion_rest_secs);
        let until = self.now_unix.saturating_add(rest);
        if let Some(&oh) = self.by_player.get(&c.owner)
            && let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(oh)
        {
            if let Some(entry) = p.data.companions.get_mut(usize::from(c.index)) {
                entry.rest_until = until;
            }
            p.data.summoned = None;
            p.dirty_stats = true;
        }
        let arg = monster_arg(self, c.monster);
        self.tell(out, c.owner, "system.companion_fell", smallvec::smallvec![arg]);
    }

    /// The bestiary of a player: monsters defeated or tamed, by id.
    pub(crate) fn bestiary(&self, p: &PlayerComp) -> Vec<crate::types::BestiaryView> {
        let cat = &self.content.catalog;
        // By monster id (sorted): (kills, tames).
        let mut seen: std::collections::BTreeMap<&str, (u64, u64)> = std::collections::BTreeMap::new();
        for (family, key, n) in &p.growth.counters {
            let e = seen.entry(key.as_str()).or_default();
            match family.as_str() {
                "kills" => e.0 = e.0.saturating_add(*n),
                "tames" => e.1 = e.1.saturating_add(*n),
                _ => {}
            }
        }
        seen.into_iter()
            .filter(|(_, (k, t))| *k > 0 || *t > 0)
            .filter_map(|(id, (kills, tames))| {
                let (_, d) = cat.monster_by_name(id)?;
                Some(crate::types::BestiaryView {
                    name: d.name,
                    icon: cat.icon_index(&d.icon),
                    level: d.level,
                    kills,
                    tames,
                })
            })
            .collect()
    }

    /// The companions window of a player.
    pub(crate) fn companion_views(&self, p: &PlayerComp) -> Vec<CompanionView> {
        let cat = &self.content.catalog;
        p.data
            .companions
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let def = cat.monster_by_name(&c.monster).map(|(_, d)| d);
                CompanionView {
                    name: def.map_or(base::TextId::new(0), |d| d.name),
                    given: c.name.clone(),
                    icon: def.map_or(0, |d| cat.icon_index(&d.icon)),
                    level: def.map_or(0, |d| d.level),
                    out: p.data.summoned == u8::try_from(i).ok(),
                    rest_secs: u32::try_from(c.rest_until.saturating_sub(self.now_unix)).unwrap_or(u32::MAX),
                }
            })
            .collect()
    }
}

/// Whether the character meets every requirement of the list.
fn meets(p: &PlayerComp, requires: &[TameReq]) -> bool {
    let s = &p.growth.sheet;
    requires.iter().all(|r| match r {
        TameReq::Level(l) => s.level >= *l,
        TameReq::Attribute(a, v) => s.attributes.get(*a) >= *v,
        TameReq::Proficiency(id, v) => s.proficiency(*id) >= *v,
        TameReq::Title(t) => s.titles.binary_search(t).is_ok(),
        TameReq::Class(c) => s.class == Some(*c),
        TameReq::Karma(k) if *k < 0 => s.karma <= *k,
        TameReq::Karma(k) => s.karma >= *k,
        TameReq::Item { item, qty } => p.data.count(*item) >= u64::from(*qty),
    })
}

/// Every companion follows its owner and helps against the owner's target,
/// in id order.
pub(crate) fn ai(z: &mut Zone, out: &mut TickOutput) {
    let now = z.tick.0;
    let mut list: Vec<(EntityId, hecs::Entity, CompanionComp)> = z
        .world
        .query::<(&CompanionComp, &Body)>()
        .iter()
        .map(|(h, (c, b))| (b.id, h, c.clone()))
        .collect();
    list.sort_by_key(|(id, _, _)| *id);
    for (id, h, c) in list {
        let Some(&oh) = z.by_player.get(&c.owner) else {
            // The owner is gone: so is the creature.
            z.remove_entity(id);
            continue;
        };
        let (Some(pos), Some(owner_pos)) = (
            z.world.get::<&Body>(h).ok().map(|b| b.pos),
            z.world.get::<&Body>(oh).ok().map(|b| b.pos),
        ) else {
            continue;
        };
        let Some(fighter) = z.world.get::<&Fighter>(h).ok().map(|f| Fighter::clone(&f)) else {
            continue;
        };
        if fighter.stunned(now) {
            continue;
        }
        // Too far behind (a portal, a long run): it catches up at once.
        if pos.distance(owner_pos) > CALL_BACK_DISTANCE {
            let to = z.free_near(owner_pos);
            if z.free(to)
                && let Ok((body, walk)) = z.world.query_one_mut::<(&mut Body, &mut Walk)>(h)
            {
                z.grid.moved(body.id, body.pos, to);
                body.pos = to;
                body.moved_tick = now;
                walk.steps.clear();
            }
            continue;
        }
        let Some(def) = z.content.catalog.monster(c.monster) else {
            continue;
        };
        let (range, attack_ticks) = (def.range, def.attack_ticks);
        let target = z
            .world
            .get::<&Fighter>(oh)
            .ok()
            .and_then(|f| f.target)
            .filter(|t| {
                z.body_of(*t).is_some_and(|(th, tpos)| {
                    z.world.get::<&MonsterComp>(th).is_ok()
                        && z.world.get::<&Fighter>(th).is_ok_and(|f| f.alive())
                        && tpos.distance(owner_pos) <= ASSIST_DISTANCE
                })
            });
        if let Ok(f) = z.world.query_one_mut::<&mut Fighter>(h) {
            f.target = target;
        }
        match target {
            Some(t) => match z.can_hit(id, t, range) {
                Ok(()) => {
                    if let Ok(w) = z.world.query_one_mut::<&mut Walk>(h) {
                        w.steps.clear();
                    }
                    if now >= fighter.next_attack {
                        if let Ok(f) = z.world.query_one_mut::<&mut Fighter>(h) {
                            f.next_attack = now.saturating_add(f.paced(u64::from(attack_ticks), now));
                        }
                        crate::status::blow(
                            z,
                            id,
                            t,
                            &crate::status::Hit {
                                mult: 1000,
                                technique: None,
                                area: 0,
                                status: None,
                            },
                            out,
                        );
                    }
                }
                Err(_) => {
                    if let Some((_, tpos)) = z.body_of(t) {
                        crate::fight::walk_to(z, h, pos, tpos, now);
                    }
                }
            },
            None if pos.distance(owner_pos) > 2 => crate::fight::walk_to(z, h, pos, owner_pos, now),
            None => {}
        }
    }
}
