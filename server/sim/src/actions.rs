//! Phase 4 commands of a player: attack, techniques, points, class,
//! equipment and appraisal; plus the status window (`StatsView`).

use crate::Zone;
use crate::fight::{self, Fighter};
use crate::gear::{self, Gear};
use crate::player::{PlayerData, Slot};
use crate::types::{Command, EquipSlot, Refusal, StatsView, TickOutput};
use crate::zone::{Body, PlayerComp};
use base::{Arg, Attribute, PlayerId, TextRef};
use smallvec::SmallVec;

/// Tiles from a fallen player within which a revival item reaches them
/// beside them, with one tile of slack.
pub const REVIVE_RADIUS: u32 = 2;

/// Marks the backpack slots of items with identity; an item whose slot is
/// taken goes to the first free one, and with no room it is kept as saved.
pub(crate) fn place_bag(data: &mut PlayerData, gear: &mut Gear, cat: &defs::ValidatedCatalog) {
    let mut placed = Vec::new();
    for (slot, inst) in std::mem::take(&mut gear.bag) {
        let free = |data: &PlayerData, i: usize| data.slots.get(i).is_some_and(|s| *s == Slot::Empty);
        let at = if free(data, usize::from(slot)) {
            Some(usize::from(slot))
        } else {
            (0..data.slots.len()).find(|i| free(data, *i))
        };
        match at.and_then(|i| Some((i, data.slots.get_mut(i)?))) {
            Some((i, s)) => {
                *s = Slot::Held(inst.item);
                placed.push((i as u16, inst));
            }
            None => {
                if let Some(saved) = inst.save(cat, crate::types::ItemPlace::Backpack(slot)) {
                    gear.kept.push(saved);
                }
            }
        }
    }
    gear.bag = placed;
}

impl Zone {
    /// `MoveItem` touching an item with identity; `None` when neither slot
    /// holds one (plain stacks go to `PlayerData::move_item`).
    pub(crate) fn move_held(&mut self, h: hecs::Entity, from: u16, to: u16) -> Option<Result<(), Refusal>> {
        let Ok((p, gear)) = self.world.query_one_mut::<(&mut PlayerComp, &mut Gear)>(h) else {
            return None;
        };
        let (a, b) = (usize::from(from), usize::from(to));
        let held = |s: Option<&Slot>| matches!(s, Some(Slot::Held(_)));
        if !held(p.data.slots.get(a)) && !held(p.data.slots.get(b)) {
            return None;
        }
        if a == b || a >= p.data.slots.len() || b >= p.data.slots.len() {
            return Some(Err(Refusal::InvalidSlot));
        }
        if matches!(p.data.slots.get(b), Some(Slot::Unknown(_))) {
            return Some(Err(Refusal::InvalidSlot));
        }
        p.data.slots.swap(a, b);
        for (slot, _) in &mut gear.bag {
            if *slot == from {
                *slot = to;
            } else if *slot == to {
                *slot = from;
            }
        }
        p.dirty_bag = true;
        Some(Ok(()))
    }

    pub(crate) fn apply_fight(
        &mut self,
        cmd: &Command,
        out: &mut TickOutput,
    ) -> Result<(), (PlayerId, Refusal)> {
        let player = match cmd {
            Command::ItemIds { start, end } => {
                self.item_ids = Some(base::IdBlock::new(*start, *end));
                return Ok(());
            }
            Command::Attack { player, .. }
            | Command::UseTechnique { player, .. }
            | Command::AllocatePoints { player, .. }
            | Command::ChooseClass { player, .. }
            | Command::Equip { player, .. }
            | Command::Unequip { player, .. }
            | Command::Appraise { player, .. }
            | Command::UseItem { player, .. }
            | Command::UseItemOn { player, .. }
            | Command::GiveUp { player } => *player,
            _ => return Ok(()),
        };
        let Some(&h) = self.by_player.get(&player) else {
            return Ok(());
        };
        let fail = |e| Err((player, e));
        if let Command::GiveUp { .. } = cmd {
            let fallen = self
                .world
                .get::<&Fighter>(h)
                .is_ok_and(|f| f.fallen_until != 0 && f.fallen_until != u64::MAX);
            if fallen {
                fight::die(self, h, out);
            }
            return Ok(());
        }
        if !self.standing(h) {
            return fail(Refusal::Fallen);
        }
        let content = self.content.clone();
        let cat = &content.catalog;
        let now = self.tick.0;
        match cmd {
            Command::Attack { target, .. } => {
                let me = self
                    .world
                    .get::<&Body>(h)
                    .map_err(|_| (player, Refusal::InvalidTarget))?
                    .id;
                // Range is checked every swing; here only who may be fought.
                match self.can_hit(me, *target, u16::MAX) {
                    Ok(()) | Err(Refusal::TooFar) => {}
                    Err(e) => return fail(e),
                }
                if let Ok(f) = self.world.query_one_mut::<&mut Fighter>(h) {
                    f.target = Some(*target);
                    f.retaliating = false;
                }
            }
            Command::UseTechnique {
                technique, target, ..
            } => {
                let Some(def) = cat.technique(*technique) else {
                    return fail(Refusal::NotReady);
                };
                let me = self
                    .world
                    .get::<&Body>(h)
                    .map_err(|_| (player, Refusal::InvalidTarget))?
                    .id;
                let (level, known, ready, vigor, mp, gcd) = {
                    let p = self
                        .world
                        .get::<&PlayerComp>(h)
                        .map_err(|_| (player, Refusal::NotReady))?;
                    let f = self
                        .world
                        .get::<&Fighter>(h)
                        .map_err(|_| (player, Refusal::NotReady))?;
                    let known = p.growth.sheet.technique(*technique).map(|t| t.level);
                    // A piece's power lends a technique at level 1 while worn.
                    let lent = self
                        .world
                        .get::<&Gear>(h)
                        .is_ok_and(|g| g.lent_techniques().contains(technique));
                    let level = known.or(lent.then_some(1));
                    (
                        level,
                        known.is_some(),
                        p.growth.ready_at(*technique),
                        f.vigor,
                        f.mp,
                        f.gcd_until,
                    )
                };
                let Some(level) = level else {
                    return fail(Refusal::NotReady);
                };
                // A thrust needs a spear in hand, a punch needs empty hands;
                // magic needs no weapon.
                let magic = cat
                    .proficiency(def.proficiency)
                    .is_some_and(|p| p.kind == defs::ProficiencyKind::Magic);
                if !magic && fight::wielded(self, h) != Some(def.proficiency) {
                    return fail(Refusal::RequirementsNotMet);
                }
                if now < ready || now < gcd || vigor < def.vigor || mp < def.mp {
                    return fail(Refusal::NotReady);
                }
                if def.damage_permille == 0 {
                    self.support(h, player, (*technique, level, known), *target, out)
                        .map_err(|e| (player, e))?;
                    out.supports.push((me, *target, *technique));
                    return Ok(());
                }
                self.can_hit(me, *target, def.range).map_err(|e| (player, e))?;
                let gcd_ticks = u64::from(cat.formulas().combat.global_cooldown_ticks);
                if let Ok((p, f)) = self.world.query_one_mut::<(&mut PlayerComp, &mut Fighter)>(h) {
                    f.vigor -= def.vigor;
                    f.mp -= def.mp;
                    f.gcd_until = now + gcd_ticks;
                    f.target = Some(*target);
                    f.retaliating = false;
                    f.last_order = now;
                    p.growth
                        .set_cooldown(*technique, now + u64::from(def.cooldown_ticks));
                    p.dirty_stats = true;
                }
                let mult = def.damage_permille.saturating_add(
                    def.growth_permille
                        .saturating_mul(u32::from(level.saturating_sub(1))),
                );
                if fight::strike(self, me, *target, mult, Some(*technique), out)
                    && known
                    && let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(h)
                {
                    let changes = progression::use_technique(&mut p.growth.sheet, *technique, cat);
                    fight::announce(self, player, &changes, out);
                }
            }
            Command::AllocatePoints { points, .. } => {
                let p = self
                    .world
                    .query_one_mut::<&mut PlayerComp>(h)
                    .map_err(|_| (player, Refusal::InvalidSlot))?;
                progression::allocate(&mut p.growth.sheet, points)
                    .map_err(|_| (player, Refusal::RequirementsNotMet))?;
                p.dirty_stats = true;
                let titles = p.growth.check_titles(cat);
                fight::refresh_player(self, h);
                fight::announce(self, player, &titles, out);
            }
            Command::ChooseClass { class, .. } => {
                let p = self
                    .world
                    .query_one_mut::<&mut PlayerComp>(h)
                    .map_err(|_| (player, Refusal::InvalidSlot))?;
                let changes = progression::choose_class_and_learn(&mut p.growth.sheet, *class, cat)
                    .map_err(|_| (player, Refusal::RequirementsNotMet))?;
                p.dirty_stats = true;
                fight::announce(self, player, &changes, out);
            }
            Command::Equip { slot, .. } => {
                let (p, gear) = self
                    .world
                    .query_one_mut::<(&mut PlayerComp, &mut Gear)>(h)
                    .map_err(|_| (player, Refusal::InvalidSlot))?;
                let Some(inst) = gear.take_bag(*slot) else {
                    return fail(Refusal::InvalidSlot);
                };
                let Some(eq) = gear::slot_of(cat.item(inst.item).and_then(|d| d.equip.as_ref())) else {
                    gear.bag.push((*slot, inst));
                    return fail(Refusal::InvalidSlot);
                };
                let s = p.data.slots.get_mut(usize::from(*slot));
                let old = gear.put_equipped(eq, inst);
                if let Some(s) = s {
                    *s = match &old {
                        Some(o) => Slot::Held(o.item),
                        None => Slot::Empty,
                    };
                }
                if let Some(o) = old {
                    gear.bag.push((*slot, o));
                }
                p.dirty_bag = true;
                p.dirty_stats = true;
                fight::refresh_player(self, h);
            }
            Command::Unequip { slot, .. } => {
                let (p, gear) = self
                    .world
                    .query_one_mut::<(&mut PlayerComp, &mut Gear)>(h)
                    .map_err(|_| (player, Refusal::InvalidSlot))?;
                if gear.slot(*slot).is_none() {
                    return fail(Refusal::InvalidSlot);
                }
                let Some(free) = p.data.slots.iter().position(|s| *s == Slot::Empty) else {
                    return fail(Refusal::InventoryFull);
                };
                if let Some(inst) = gear.take_equipped(*slot) {
                    if let Some(s) = p.data.slots.get_mut(free) {
                        *s = Slot::Held(inst.item);
                    }
                    gear.bag.push((free as u16, inst));
                }
                p.dirty_bag = true;
                p.dirty_stats = true;
                fight::refresh_player(self, h);
            }
            Command::Appraise { slot, .. } => {
                let zone_seed = self.seed;
                let (p, gear) = self
                    .world
                    .query_one_mut::<(&mut PlayerComp, &mut Gear)>(h)
                    .map_err(|_| (player, Refusal::InvalidSlot))?;
                // Only a production class appraises by itself; the others
                // pay a shop that appraises.
                if !crate::craft::is_production(&p.growth.sheet, cat) {
                    self.tell(out, player, "system.appraise_class_only", SmallVec::new());
                    return fail(Refusal::RequirementsNotMet);
                }
                let level = p.growth.sheet.level;
                let dex = p.growth.sheet.attributes.get(Attribute::Dex);
                let Some(inst) = gear.bag_item(*slot) else {
                    return fail(Refusal::InvalidSlot);
                };
                if inst.appraised {
                    return fail(Refusal::NotReady);
                }
                let ok = gear::appraisal_succeeds(inst.id, level, dex, zone_seed);
                let name = cat.item(inst.item).map(|d| d.name);
                if ok {
                    inst.appraised = true;
                    p.dirty_bag = true;
                }
                let key = if ok {
                    "system.appraised"
                } else {
                    "system.appraise_failed"
                };
                let args: SmallVec<[Arg; 2]> = name
                    .map(|n| Arg::Ref(Box::new(TextRef::new(n))))
                    .into_iter()
                    .collect();
                self.tell(out, player, key, args);
            }
            Command::UseItem { slot, .. } => {
                let (p, f) = self
                    .world
                    .query_one_mut::<(&mut PlayerComp, &mut Fighter)>(h)
                    .map_err(|_| (player, Refusal::InvalidSlot))?;
                let Some(item) = p.data.stack_at(usize::from(*slot)) else {
                    return fail(Refusal::InvalidSlot);
                };
                let Some((def, effect)) = cat.item(item).and_then(|d| Some((d, d.use_effect?))) else {
                    return fail(Refusal::InvalidSlot);
                };
                // A revival item is used on someone else (`UseItemOn`).
                if effect.revive_permille > 0 {
                    return fail(Refusal::InvalidTarget);
                }
                let ready = p
                    .item_ready
                    .iter()
                    .find(|(i, _)| *i == item)
                    .map_or(0, |(_, t)| *t);
                let gain_hp = effect.hp.min(f.max_hp - f.hp.min(f.max_hp));
                let gain_vigor = effect.vigor.min(f.max_vigor - f.vigor.min(f.max_vigor));
                let gain_mp = effect.mp.min(f.max_mp - f.mp.min(f.max_mp));
                // Resting, or nothing to restore: the item is not wasted.
                if now < ready || (gain_hp == 0 && gain_vigor == 0 && gain_mp == 0) {
                    return fail(Refusal::NotReady);
                }
                p.data.take(item, 1).map_err(|_| (player, Refusal::InvalidSlot))?;
                f.hp += gain_hp;
                f.vigor += gain_vigor;
                f.mp += gain_mp;
                let until = now + u64::from(effect.cooldown_ticks);
                match p.item_ready.binary_search_by_key(&item, |(i, _)| *i) {
                    Ok(i) => {
                        if let Some(r) = p.item_ready.get_mut(i) {
                            r.1 = until;
                        }
                    }
                    Err(i) => p.item_ready.insert(i, (item, until)),
                }
                p.dirty_bag = true;
                p.dirty_stats = true;
                let name = Arg::Ref(Box::new(TextRef::new(def.name)));
                if let Ok(b) = self.world.query_one_mut::<&mut Body>(h) {
                    b.status_tick = now;
                }
                if gain_mp > 0 && gain_hp == 0 && gain_vigor == 0 {
                    self.note(
                        out,
                        player,
                        "system.item_used_mp",
                        smallvec::smallvec![name, Arg::Int(i64::from(gain_mp))],
                    );
                } else {
                    self.note(
                        out,
                        player,
                        "system.item_used",
                        smallvec::smallvec![
                            name,
                            Arg::Int(i64::from(gain_hp)),
                            Arg::Int(i64::from(gain_vigor))
                        ],
                    );
                }
            }
            Command::UseItemOn { slot, target, .. } => {
                self.use_item_on(h, player, *slot, *target, out)
                    .map_err(|e| (player, e))?;
            }
            _ => {}
        }
        Ok(())
    }

    /// A technique that heals a standing player (or oneself) or
    /// stands a fallen one up. Costs and cooldown as any technique; the
    /// proficiency and the technique grow only when it did something.
    fn support(
        &mut self,
        h: hecs::Entity,
        player: PlayerId,
        (technique, level, known): (base::TechniqueId, u8, bool),
        target: base::EntityId,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let now = self.tick.0;
        let content = self.content.clone();
        let cat = &content.catalog;
        let def = cat.technique(technique).ok_or(Refusal::NotReady)?;
        // Support magic grows with the proficiency in use (: faith
        // adds to light).
        let points = self
            .world
            .get::<&PlayerComp>(h)
            .map(|p| progression::effective_proficiency(&p.growth.sheet, def.proficiency, cat))
            .unwrap_or(0);
        let power = cat
            .formulas()
            .combat
            .support_per_point_permille
            .saturating_mul(u32::from(points));
        if let Some(buff) = def.buff {
            return self.vow(h, player, (technique, level, known), buff, power, out);
        }
        let (other, th) = self.player_at(target).ok_or(Refusal::InvalidTarget)?;
        let (mp, tp) = (
            self.pos_of_handle(h).ok_or(Refusal::InvalidTarget)?,
            self.pos_of_handle(th).ok_or(Refusal::InvalidTarget)?,
        );
        if mp.distance(tp) > u32::from(def.range) {
            return Err(Refusal::TooFar);
        }
        let (fallen, hp, max_hp, target_level) = {
            let f = self
                .world
                .get::<&Fighter>(th)
                .map_err(|_| Refusal::InvalidTarget)?;
            let lvl = self
                .world
                .get::<&PlayerComp>(th)
                .map_err(|_| Refusal::InvalidTarget)?
                .growth
                .sheet
                .level;
            (f.fallen_until, f.hp, f.max_hp, lvl)
        };
        let grown = |base: u32| {
            base.saturating_add(
                def.growth_permille
                    .saturating_mul(u32::from(level.saturating_sub(1))),
            )
            .saturating_add(power)
            .min(1000)
        };
        if def.revive_permille > 0 {
            if other == player || fallen == 0 || fallen == u64::MAX {
                return Err(Refusal::InvalidTarget);
            }
        } else if fallen != 0 {
            return Err(Refusal::InvalidTarget);
        } else if hp >= max_hp {
            // Nothing to heal: nothing spent.
            return Err(Refusal::NotReady);
        }
        let gcd_ticks = u64::from(cat.formulas().combat.global_cooldown_ticks);
        if let Ok((p, f)) = self.world.query_one_mut::<(&mut PlayerComp, &mut Fighter)>(h) {
            f.vigor -= def.vigor;
            f.mp -= def.mp;
            f.gcd_until = now + gcd_ticks;
            f.last_order = now;
            p.growth
                .set_cooldown(technique, now + u64::from(def.cooldown_ticks));
            p.dirty_stats = true;
        }
        let (me, them) = (self.name_of(player), self.name_of(other));
        if def.revive_permille > 0 {
            self.stand_up(th, def.revive_permille.min(1000) as u16, now);
            self.tell(out, other, "system.revived_by", smallvec::smallvec![Arg::Str(me)]);
            self.note(
                out,
                player,
                "system.revive_done",
                smallvec::smallvec![Arg::Str(them)],
            );
        } else {
            let gain = (u64::from(max_hp) * u64::from(grown(def.heal_permille)) / 1000).max(1);
            let gain = u32::try_from(gain).unwrap_or(u32::MAX).min(max_hp - hp);
            if let Ok((f, p, b)) = self
                .world
                .query_one_mut::<(&mut Fighter, &mut PlayerComp, &mut Body)>(th)
            {
                f.hp += gain;
                p.dirty_stats = true;
                b.status_tick = now;
            }
            if other != player {
                self.note(
                    out,
                    other,
                    "system.healed_by",
                    smallvec::smallvec![Arg::Str(me), Arg::Int(i64::from(gain))],
                );
            }
        }
        // Growth by use with effect.
        if let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(h) {
            let mut changes =
                progression::use_proficiency(&mut p.growth.sheet, def.proficiency, target_level, now, cat);
            if known {
                changes.extend(progression::use_technique(&mut p.growth.sheet, technique, cat));
            }
            if changes
                .iter()
                .any(|c| matches!(c, progression::Change::ProficiencyRaised(..)))
            {
                changes.extend(p.growth.check_titles(cat));
            }
            if !changes.is_empty() {
                p.dirty_stats = true;
                fight::announce(self, player, &changes, out);
            }
        }
        Ok(())
    }

    /// A buff on the caster (the paladin's vow). Costs and
    /// cooldown as any technique; it always does something, so it always
    /// grows the proficiency and the technique.
    fn vow(
        &mut self,
        h: hecs::Entity,
        player: PlayerId,
        (technique, level, known): (base::TechniqueId, u8, bool),
        buff: defs::Buff,
        power: u32,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let now = self.tick.0;
        let content = self.content.clone();
        let cat = &content.catalog;
        let def = cat.technique(technique).ok_or(Refusal::NotReady)?;
        let permille = buff
            .permille
            .saturating_add(
                def.growth_permille
                    .saturating_mul(u32::from(level.saturating_sub(1))),
            )
            .saturating_add(power);
        let gcd_ticks = u64::from(cat.formulas().combat.global_cooldown_ticks);
        let my_level = {
            let (p, f, b) = self
                .world
                .query_one_mut::<(&mut PlayerComp, &mut Fighter, &mut Body)>(h)
                .map_err(|_| Refusal::NotReady)?;
            f.vigor -= def.vigor;
            f.mp -= def.mp;
            f.gcd_until = now + gcd_ticks;
            f.last_order = now;
            match buff.stat {
                defs::BuffStat::Defense => {
                    f.ward = fight::Ward {
                        permille,
                        flat: buff.flat,
                        until: now + u64::from(buff.duration_ticks),
                    };
                }
            }
            p.growth
                .set_cooldown(technique, now + u64::from(def.cooldown_ticks));
            p.dirty_stats = true;
            b.status_tick = now;
            p.growth.sheet.level
        };
        let name = Arg::Ref(Box::new(TextRef::new(def.name)));
        let secs = i64::try_from(u64::from(buff.duration_ticks) / base::TICKS_PER_SECOND).unwrap_or(0);
        self.note(
            out,
            player,
            "system.vow_taken",
            smallvec::smallvec![name, Arg::Int(secs)],
        );
        if let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(h) {
            let mut changes =
                progression::use_proficiency(&mut p.growth.sheet, def.proficiency, my_level, now, cat);
            if known {
                changes.extend(progression::use_technique(&mut p.growth.sheet, technique, cat));
            }
            if changes
                .iter()
                .any(|c| matches!(c, progression::Change::ProficiencyRaised(..)))
            {
                changes.extend(p.growth.check_titles(cat));
            }
            if !changes.is_empty() {
                p.dirty_stats = true;
                fight::announce(self, player, &changes, out);
            }
        }
        Ok(())
    }

    /// A fallen player stands up with these thousandths of their HP, with
    /// no loss.
    fn stand_up(&mut self, th: hecs::Entity, permille: u16, now: u64) {
        if let Ok((f, p, b)) = self
            .world
            .query_one_mut::<(&mut Fighter, &mut PlayerComp, &mut Body)>(th)
        {
            f.fallen_until = 0;
            let share = u64::from(f.max_hp) * u64::from(permille) / 1000;
            f.hp = u32::try_from(share).unwrap_or(f.max_hp).clamp(1, f.max_hp.max(1));
            p.dirty_stats = true;
            b.status_tick = now;
        }
    }

    /// A revival item used on a fallen player within
    /// `REVIVE_RADIUS`; they stand up with a share of their HP, with no
    /// loss. The item rests like any consumable.
    fn use_item_on(
        &mut self,
        h: hecs::Entity,
        player: PlayerId,
        slot: u16,
        target: base::EntityId,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let now = self.tick.0;
        let content = self.content.clone();
        let (other, th) = self.player_at(target).ok_or(Refusal::InvalidTarget)?;
        let fallen = self
            .world
            .get::<&Fighter>(th)
            .is_ok_and(|f| f.fallen_until != 0 && f.fallen_until != u64::MAX);
        if other == player || !fallen {
            return Err(Refusal::InvalidTarget);
        }
        let (mp, tp) = (
            self.pos_of_handle(h).ok_or(Refusal::InvalidTarget)?,
            self.pos_of_handle(th).ok_or(Refusal::InvalidTarget)?,
        );
        if mp.distance(tp) > REVIVE_RADIUS {
            return Err(Refusal::TooFar);
        }
        let item = {
            let p = self
                .world
                .get::<&PlayerComp>(h)
                .map_err(|_| Refusal::InvalidSlot)?;
            p.data.stack_at(usize::from(slot)).ok_or(Refusal::InvalidSlot)?
        };
        let def = content.catalog.item(item).ok_or(Refusal::InvalidSlot)?;
        let effect = def
            .use_effect
            .filter(|u| u.revive_permille > 0)
            .ok_or(Refusal::InvalidSlot)?;
        {
            let p = self
                .world
                .query_one_mut::<&mut PlayerComp>(h)
                .map_err(|_| Refusal::InvalidSlot)?;
            let ready = p
                .item_ready
                .iter()
                .find(|(i, _)| *i == item)
                .map_or(0, |(_, t)| *t);
            if now < ready {
                return Err(Refusal::NotReady);
            }
            p.data.take(item, 1).map_err(|_| Refusal::InvalidSlot)?;
            let until = now + u64::from(effect.cooldown_ticks);
            match p.item_ready.binary_search_by_key(&item, |(i, _)| *i) {
                Ok(i) => {
                    if let Some(r) = p.item_ready.get_mut(i) {
                        r.1 = until;
                    }
                }
                Err(i) => p.item_ready.insert(i, (item, until)),
            }
            p.dirty_bag = true;
            p.dirty_stats = true;
        }
        self.stand_up(th, effect.revive_permille, now);
        let (me, them) = (self.name_of(player), self.name_of(other));
        self.tell(out, other, "system.revived_by", smallvec::smallvec![Arg::Str(me)]);
        self.note(
            out,
            player,
            "system.revive_done",
            smallvec::smallvec![Arg::Str(them)],
        );
        Ok(())
    }

    /// The status window of a player whose sheet changed (clears the flag).
    pub(crate) fn stats_view(&mut self, h: hecs::Entity) -> Option<StatsView> {
        let now = self.tick.0;
        let content = self.content.clone();
        let cat = &content.catalog;
        let (p, f, gear) = self
            .world
            .query_one_mut::<(&mut PlayerComp, &Fighter, &Gear)>(h)
            .ok()?;
        if !p.dirty_stats {
            return None;
        }
        p.dirty_stats = false;
        let s = &p.growth.sheet;
        Some(StatsView {
            level: s.level,
            xp: s.xp,
            xp_next: cat.formulas().level_xp(s.level),
            hp: f.hp,
            max_hp: f.max_hp,
            vigor: f.vigor,
            max_vigor: f.max_vigor,
            mp: f.mp,
            max_mp: f.max_mp,
            attributes: p.growth.attributes(&gear.bonus(), cat),
            base_attributes: s.attributes,
            appraises: crate::craft::is_production(s, cat),
            free_points: s.free_points,
            class: s.class,
            classes_available: progression::available_classes(s, cat),
            proficiencies: s.proficiencies.iter().map(|x| (x.id, x.points)).collect(),
            techniques: {
                let mut t: Vec<_> = s.techniques.iter().map(|x| (x.id, x.level)).collect();
                // What the worn pieces lend shows like a known technique.
                for lent in gear.lent_techniques() {
                    if !t.iter().any(|(id, _)| *id == lent) {
                        t.push((lent, 1));
                    }
                }
                t
            },
            titles: s.titles.clone(),
            equipment: EquipSlot::ALL
                .iter()
                .filter_map(|e| Some((*e, gear.slot(*e)?.item)))
                .collect(),
            defense: f.ward.defense(gear.defense(cat), now),
            fallen: f.fallen_until != 0,
            cooldowns: p
                .growth
                .cooldowns
                .iter()
                .filter(|(_, r)| *r > now)
                .map(|(t, r)| (*t, u32::try_from(r - now).unwrap_or(u32::MAX)))
                .collect(),
            global_cooldown: u32::try_from(f.gcd_until.saturating_sub(now)).unwrap_or(u32::MAX),
            recipes: p.data.recipes.clone(),
            specials: progression::special_attributes(s, cat)
                .into_iter()
                .filter_map(|(i, v)| Some((cat.special_attributes().get(i)?.name, v)))
                .collect(),
        })
    }

    /// Does `player` currently see `entity`?
    pub fn knows(&self, player: PlayerId, entity: base::EntityId) -> bool {
        self.by_player
            .get(&player)
            .and_then(|h| self.world.get::<&PlayerComp>(*h).ok())
            .is_some_and(|p| p.known.binary_search(&entity).is_ok())
    }

    /// Item ids left in the block (0 without one).
    pub fn item_ids_left(&self) -> u64 {
        self.item_ids.as_ref().map_or(0, base::IdBlock::remaining)
    }

    /// Entities of one monster kind in the zone, by id.
    pub fn monster_ids(&self, monster: base::MonsterId) -> Vec<base::EntityId> {
        self.by_id
            .iter()
            .filter(|(_, h)| {
                self.world
                    .get::<&crate::fight::MonsterComp>(**h)
                    .is_ok_and(|m| m.monster == monster)
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Every entry of `monsters.ron`, `count` times each.
    pub(crate) fn spawn_all_monsters(&mut self) {
        let spawns: Vec<(u16, u16)> = self
            .monster_spawns
            .iter()
            .enumerate()
            .map(|(i, s)| (i as u16, s.count))
            .collect();
        for (i, count) in spawns {
            for _ in 0..count {
                self.spawn_monster(i);
            }
        }
    }
}
