//! Gathering and crafting (phase 8).
//!
//! A resource node empties **per player**: the node remembers who
//! gathered it and the tick it comes back for them, so nobody disputes a
//! node and no bot can camp one.
//!
//! Crafting is instant and always allowed for a recipe the character
//! knows; what the production class changes is the level XP and the best
//! rarity that may come out.

use crate::Zone;
use crate::gear::Gear;
use crate::player::Slot;
use crate::types::{Refusal, TickOutput};
use crate::zone::{Body, PlayerComp, TALK_RADIUS};
use base::{Arg, EntityId, ItemId, NodeId, PlayerId, Rarity, RecipeId, Rng64, TextRef};
use defs::{LootTableDef, ValidatedCatalog};
use serde::{Deserialize, Serialize};

/// A resource node standing in the zone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NodeComp {
    pub node: NodeId,
    /// Players who emptied it, with the tick it comes back for them.
    /// Sorted by player; entries whose tick passed are dropped on sight,
    /// so the list stays as short as the players standing around it.
    pub used: Vec<(PlayerId, u64)>,
}

impl NodeComp {
    pub fn new(node: NodeId) -> NodeComp {
        NodeComp {
            node,
            used: Vec::new(),
        }
    }

    /// Is the node full for this player right now?
    fn ready(&self, player: PlayerId, now: u64) -> bool {
        self.used
            .binary_search_by_key(&player, |(p, _)| *p)
            .ok()
            .and_then(|i| self.used.get(i))
            .is_none_or(|(_, until)| now >= *until)
    }

    fn empty_for(&mut self, player: PlayerId, until: u64, now: u64) {
        self.used.retain(|(p, u)| *p == player || *u > now);
        match self.used.binary_search_by_key(&player, |(p, _)| *p) {
            Ok(i) => {
                if let Some(e) = self.used.get_mut(i) {
                    e.1 = until;
                }
            }
            Err(i) => self.used.insert(i, (player, until)),
        }
    }
}

/// Rarity one step above `r`, up to `cap`.
fn next_rarity(r: Rarity) -> Option<Rarity> {
    Some(match r {
        Rarity::Common => Rarity::Rare,
        Rarity::Rare => Rarity::Epic,
        Rarity::Epic => Rarity::Unique,
        Rarity::Unique => Rarity::Legendary,
        Rarity::Legendary => Rarity::Mythic,
        Rarity::Mythic => return None,
    })
}

/// Rarity of a craft: starts at the item's own and climbs while the roll
/// holds, never past the class's cap. Each step halves the chance.
pub(crate) fn roll_quality(
    base: Rarity,
    cap: Rarity,
    points: u8,
    dex: u16,
    cat: &ValidatedCatalog,
    rng: &mut Rng64,
) -> Rarity {
    let e = &cat.formulas().economy;
    let mut chance = e
        .craft_quality_permille
        .saturating_add(e.craft_quality_per_point_permille * u32::from(points))
        .saturating_add(e.craft_quality_per_dex_permille * u32::from(dex))
        .min(e.craft_quality_max_permille);
    let mut rarity = base;
    while rarity < cap {
        if chance == 0 || !rng.chance(chance) {
            break;
        }
        match next_rarity(rarity) {
            Some(r) => rarity = r,
            None => break,
        }
        chance /= 2;
    }
    rarity
}

/// The class lifts the crafting quality cap: a production class. It has
/// mastery, crafts anywhere and appraises by itself.
pub(crate) fn is_production(sheet: &progression::ProgressionSheet, cat: &ValidatedCatalog) -> bool {
    sheet
        .class
        .and_then(|c| cat.class(c))
        .is_some_and(|c| c.quality_cap > Rarity::Common)
}

impl Zone {
    /// Crafts one batch of a recipe the character knows.
    pub(crate) fn craft(
        &mut self,
        player: PlayerId,
        recipe: RecipeId,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let handle = *self.by_player.get(&player).ok_or(Refusal::CannotCraft)?;
        let content = self.content.clone();
        let cat = &content.catalog;
        let def = cat.recipe(recipe).ok_or(Refusal::CannotCraft)?;
        let idef = cat.item(def.result).ok_or(Refusal::CannotCraft)?;
        let equipment = idef.equip.is_some();
        let (points, dex, cap, tier, xp_permille) = {
            let p = self
                .world
                .get::<&PlayerComp>(handle)
                .map_err(|_| Refusal::CannotCraft)?;
            if !p.data.knows(recipe) {
                drop(p);
                self.tell(out, player, "system.craft_unknown", smallvec::SmallVec::new());
                return Err(Refusal::CannotCraft);
            }
            let sheet = &p.growth.sheet;
            let points = sheet.proficiency(def.proficiency);
            if points < def.min_points {
                drop(p);
                let prof = cat.proficiency(def.proficiency).map(|d| d.name);
                self.tell(
                    out,
                    player,
                    "system.craft_points",
                    smallvec::smallvec![
                        prof.map_or(Arg::Int(0), |n| Arg::Ref(Box::new(TextRef::new(n)))),
                        Arg::Int(i64::from(def.min_points))
                    ],
                );
                return Err(Refusal::RequirementsNotMet);
            }
            let class = sheet.class.and_then(|c| cat.class(c));
            let economy = &cat.formulas().economy;
            (
                points,
                sheet.attributes.get(base::Attribute::Dex),
                class.map_or(Rarity::Common, |c| c.quality_cap),
                // Only a production class (one that lifts the quality cap)
                // has mastery.
                class
                    .filter(|c| c.quality_cap > Rarity::Common)
                    .map_or(0, |c| c.tier),
                match class.map_or(0, |c| c.craft_xp_permille) {
                    0 => economy.craft_xp_permille,
                    n => n,
                },
            )
        };
        // Outside a production class, crafting needs a workshop.
        if tier == 0 {
            let pos = self
                .world
                .get::<&Body>(handle)
                .map(|b| b.pos)
                .map_err(|_| Refusal::CannotCraft)?;
            if !self.in_workshop(pos) {
                self.tell(
                    out,
                    player,
                    "system.craft_needs_workshop",
                    smallvec::SmallVec::new(),
                );
                return Err(Refusal::CannotCraft);
            }
        }
        // Equipment is a piece with identity: it needs a free slot and an id.
        if equipment && self.item_ids.as_ref().is_none_or(|b| b.remaining() == 0) {
            out.need_item_ids = true;
            self.tell(out, player, "system.craft_busy", smallvec::SmallVec::new());
            return Err(Refusal::CannotCraft);
        }
        // Materials come out in a copy: a craft that cannot be finished
        // changes nothing.
        let mut data = self
            .world
            .get::<&PlayerComp>(handle)
            .map(|p| p.data.clone())
            .map_err(|_| Refusal::CannotCraft)?;
        for m in &def.materials {
            if data.take(m.item, m.qty).is_err() {
                // Says which one: the window counts the whole backpack, and
                // a piece with identity (worn, or a weapon) is not a material.
                let have = self
                    .world
                    .get::<&PlayerComp>(handle)
                    .map_or(0, |p| p.data.count(m.item));
                let mut args: smallvec::SmallVec<[Arg; 2]> = smallvec::smallvec![
                    cat.item(m.item)
                        .map_or(Arg::Int(0), |d| Arg::Ref(Box::new(TextRef::new(d.name)))),
                    Arg::Int(i64::from(m.qty))
                ];
                args.push(Arg::Int(i64::try_from(have).unwrap_or(i64::MAX)));
                self.tell(out, player, "system.craft_material", args);
                return Err(Refusal::CannotCraft);
            }
        }
        if equipment {
            if !data.slots.contains(&Slot::Empty) {
                return Err(Refusal::InventoryFull);
            }
        } else {
            data.give(cat, def.result, def.qty)
                .map_err(|_| Refusal::InventoryFull)?;
        }
        let name = TextRef::new(idef.name);
        let rarity = roll_quality(idef.rarity, cap, points, dex, cat, &mut self.rng);
        // A production class's mastery: tiny for a new artisan, up to the
        // cap with points, DEX and a higher class.
        let mastery = cat.formulas().economy.mastery(points, dex, tier);
        if let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(handle) {
            p.data = data;
            p.dirty_bag = true;
        }
        if equipment {
            let who = self
                .world
                .get::<&PlayerComp>(handle)
                .ok()
                .map(|p| p.sheet.name.clone());
            crate::fight::give_instance(self, handle, def.result, out);
            // Every crafted piece carries its maker and its rolled rarity
            if let Ok(gear) = self.world.query_one_mut::<&mut Gear>(handle)
                && let Some((_, inst)) = gear.bag.last_mut()
            {
                inst.crafter = who;
                inst.rarity = rarity;
                inst.mastery = mastery;
                // Durability and power follow the rolled rarity and mastery.
                inst.shape(cat, &mut self.rng);
            }
        }
        self.reward_work(handle, player, def.proficiency, def.xp, xp_permille, out);
        self.tell(
            out,
            player,
            "system.crafted",
            smallvec::smallvec![Arg::Ref(Box::new(name)), Arg::Int(i64::from(def.qty))],
        );
        Ok(())
    }

    /// Gathers a node within reach.
    pub(crate) fn gather(
        &mut self,
        player: PlayerId,
        target: EntityId,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let handle = *self.by_player.get(&player).ok_or(Refusal::NothingToGather)?;
        let pos = self
            .world
            .get::<&Body>(handle)
            .map(|b| b.pos)
            .map_err(|_| Refusal::NothingToGather)?;
        let th = *self.by_id.get(&target).ok_or(Refusal::NothingToGather)?;
        let node_pos = self
            .world
            .get::<&Body>(th)
            .map(|b| b.pos)
            .map_err(|_| Refusal::NothingToGather)?;
        if pos.distance(node_pos) > TALK_RADIUS {
            return Err(Refusal::TooFar);
        }
        let now = self.tick.0;
        let node = self
            .world
            .get::<&NodeComp>(th)
            .ok()
            .filter(|n| n.ready(player, now))
            .map(|n| n.node)
            .ok_or(Refusal::NothingToGather)?;
        let content = self.content.clone();
        let cat = &content.catalog;
        let def = cat.node(node).ok_or(Refusal::NothingToGather)?;
        let points = self
            .world
            .get::<&PlayerComp>(handle)
            .map(|p| p.growth.sheet.proficiency(def.proficiency))
            .unwrap_or(0);
        if points < def.min_points {
            return Err(Refusal::RequirementsNotMet);
        }
        let table = cat.loot_table(def.loot).ok_or(Refusal::NothingToGather)?;
        let cooldown = if def.cooldown_ticks == 0 {
            cat.formulas().economy.gather_cooldown_ticks
        } else {
            def.cooldown_ticks
        };
        let (prof, xp) = (def.proficiency, def.xp);
        let (coins, got) = self.roll_table(table);
        if let Ok(n) = self.world.query_one_mut::<&mut NodeComp>(th) {
            n.empty_for(player, now + u64::from(cooldown), now);
        }
        crate::fight::give_loot(self, handle, coins, &got, crate::types::CoinFlow::Gift, out);
        // Gathering is the work itself: it always pays its own XP in full.
        self.reward_work(handle, player, prof, xp, 1000, out);
        if let Ok(b) = self.world.query_one_mut::<&mut Body>(th) {
            // The node changed for somebody: let the view refresh it.
            b.status_tick = now;
        }
        Ok(())
    }

    /// Switches a tile of this zone within reach: the floor and
    /// its `switch` trade places for everyone; what blocks stays the same,
    /// so no path or sight changes. A tile of the neighbor's band belongs
    /// to the neighbor.
    pub(crate) fn use_tile(
        &mut self,
        player: PlayerId,
        pos: base::TilePos,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let handle = *self.by_player.get(&player).ok_or(Refusal::NothingToUse)?;
        let me = self
            .world
            .get::<&Body>(handle)
            .map(|b| b.pos)
            .map_err(|_| Refusal::NothingToUse)?;
        let own = self
            .content
            .catalog
            .zone(self.zone_id())
            .is_some_and(|z| z.tile_at(pos).is_some_and(|t| t.switch.is_some()));
        if !own {
            return Err(Refusal::NothingToUse);
        }
        if me.distance(pos) > TALK_RADIUS {
            return Err(Refusal::TooFar);
        }
        let word = self.map.switch(pos).ok_or(Refusal::NothingToUse)?;
        out.tiles.push((pos, word));
        Ok(())
    }

    /// Rolls a loot table: coins and `(item, pieces)` lines.
    pub(crate) fn roll_table(&mut self, table: &LootTableDef) -> (u64, Vec<(ItemId, u32)>) {
        let coins = u64::from(self.rng.range(table.coins.0, table.coins.1));
        let mut got = Vec::new();
        for e in &table.entries {
            if self.rng.chance(u32::from(e.chance)) {
                got.push((e.item, self.rng.range(u32::from(e.qty.0), u32::from(e.qty.1))));
            }
        }
        (coins, got)
    }

    /// XP of a day's work: level XP scaled by the class and the
    /// proficiency raised by use.
    pub(crate) fn reward_work(
        &mut self,
        handle: hecs::Entity,
        player: PlayerId,
        prof: base::ProficiencyId,
        xp: u64,
        xp_permille: u32,
        out: &mut TickOutput,
    ) {
        let content = self.content.clone();
        let cat = &content.catalog;
        let now = self.tick.0;
        let level_xp = xp.saturating_mul(u64::from(xp_permille)) / 1000;
        let mut changes = Vec::new();
        if let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(handle) {
            let level = p.growth.sheet.level;
            changes.extend(progression::use_proficiency(
                &mut p.growth.sheet,
                prof,
                level,
                now,
                cat,
            ));
            if level_xp > 0 {
                changes.extend(progression::add_xp(&mut p.growth.sheet, level_xp, cat));
            }
            changes.extend(p.growth.check_titles(cat));
            p.dirty_stats = true;
        }
        if changes.iter().any(|c| {
            matches!(
                c,
                progression::Change::LeveledUp(_) | progression::Change::TitleEarned(_)
            )
        }) {
            crate::fight::refresh_player(self, handle);
        }
        if level_xp > 0 {
            self.note(
                out,
                player,
                "system.xp",
                smallvec::smallvec![Arg::Int(i64::try_from(level_xp).unwrap_or(i64::MAX))],
            );
        }
        crate::fight::announce(self, player, &changes, out);
    }
}
