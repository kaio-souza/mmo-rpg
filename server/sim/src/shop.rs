//! NPC shops (phase 7): buying, selling and repairing
//!
//! The shop's prices are not in the shop: they come from each item's
//! `value` and from `economy/formulas.ron`, with the city's variation
//! (`price_permille`) on top, so a balance change is one file.
//!
//! Stock restocks **lazily**: nothing runs per tick. Whoever opens the
//! shop pays for the arithmetic of the ticks that passed, which keeps the
//! result the same on any machine and costs nothing while nobody is
//! looking.

use crate::Zone;
use crate::gear::Gear;
use crate::player::Slot;
use crate::types::{CoinFlow, Refusal, ShopLineView, ShopView, TickOutput};
use crate::zone::{Body, NpcComp, PlayerComp, TALK_RADIUS};
use base::{Arg, EntityId, ItemId, PlayerId, ShopId, TextRef};
use defs::{Purse, ShopDef, ValidatedCatalog};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// A shop as the zone keeps it: one line per entry of the definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ShopComp {
    pub shop: ShopId,
    pub lines: Vec<Line>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Line {
    /// Pieces left; `u32::MAX` means the line never runs out.
    pub left: u32,
    /// Tick the restock clock was last settled at.
    pub settled: u64,
}

/// A line with no limit (`qty: 0` in the file).
const UNLIMITED: u32 = u32::MAX;

impl ShopComp {
    pub fn new(shop: ShopId, def: &ShopDef, now: u64) -> ShopComp {
        ShopComp {
            shop,
            lines: def
                .stock
                .iter()
                .map(|s| Line {
                    left: if s.qty == 0 { UNLIMITED } else { s.qty },
                    settled: now,
                })
                .collect(),
        }
    }

    /// Puts back the pieces the ticks since `settled` are worth.
    fn restock(&mut self, def: &ShopDef, now: u64) {
        for (line, stock) in self.lines.iter_mut().zip(def.stock.iter()) {
            if line.left == UNLIMITED || stock.restock_ticks == 0 || stock.qty == 0 {
                line.settled = now;
                continue;
            }
            let ticks = now.saturating_sub(line.settled);
            let back = ticks / u64::from(stock.restock_ticks);
            if back > 0 {
                let back = u32::try_from(back).unwrap_or(u32::MAX);
                line.left = line.left.saturating_add(back).min(stock.qty);
                line.settled = line
                    .settled
                    .saturating_add(back as u64 * u64::from(stock.restock_ticks));
            }
            if line.left >= stock.qty {
                line.settled = now;
            }
        }
    }
}

/// The shop a player has open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ShopOpen {
    pub npc: EntityId,
    pub shop: ShopId,
    /// The view has to be sent again at the end of the tick.
    pub dirty: bool,
}

impl Zone {
    /// Opens the shop of a nearby NPC.
    pub(crate) fn open_shop(&mut self, player: PlayerId, target: EntityId) -> Result<(), Refusal> {
        let handle = *self.by_player.get(&player).ok_or(Refusal::NoShop)?;
        let pos = self
            .world
            .get::<&Body>(handle)
            .map(|b| b.pos)
            .map_err(|_| Refusal::NoShop)?;
        let th = *self.by_id.get(&target).ok_or(Refusal::NoShop)?;
        let target_pos = self
            .world
            .get::<&Body>(th)
            .map(|b| b.pos)
            .map_err(|_| Refusal::NoShop)?;
        let shop = self
            .world
            .get::<&NpcComp>(th)
            .ok()
            .and_then(|n| self.content.catalog.npc(n.npc)?.shop)
            .ok_or(Refusal::NoShop)?;
        if pos.distance(target_pos) > TALK_RADIUS {
            return Err(Refusal::TooFar);
        }
        let _ = self.world.insert_one(
            handle,
            ShopOpen {
                npc: target,
                shop,
                dirty: true,
            },
        );
        Ok(())
    }

    pub(crate) fn close_shop(&mut self, player: PlayerId, out: &mut TickOutput) {
        let Some(&handle) = self.by_player.get(&player) else {
            return;
        };
        if self.world.remove_one::<ShopOpen>(handle).is_ok() {
            out.shops.push((player, None));
        }
    }

    /// Walking away closes the shop, like a dialogue.
    pub(crate) fn close_far_shops(&mut self, out: &mut TickOutput) {
        let mut far = Vec::new();
        for (&player, &handle) in &self.by_player {
            let Ok(open) = self.world.get::<&ShopOpen>(handle) else {
                continue;
            };
            let near = match (self.world.get::<&Body>(handle), self.by_id.get(&open.npc)) {
                (Ok(me), Some(&h)) => self
                    .world
                    .get::<&Body>(h)
                    .is_ok_and(|b| me.pos.distance(b.pos) <= TALK_RADIUS),
                _ => false,
            };
            if !near {
                far.push((player, handle));
            }
        }
        for (player, handle) in far {
            let _ = self.world.remove_one::<ShopOpen>(handle);
            out.shops.push((player, None));
        }
    }

    /// The open shop of a player, with the stock settled for this tick.
    fn open_of(&mut self, player: PlayerId) -> Result<(hecs::Entity, hecs::Entity, ShopId), Refusal> {
        let handle = *self.by_player.get(&player).ok_or(Refusal::NoShop)?;
        let open = self
            .world
            .get::<&ShopOpen>(handle)
            .map(|o| *o)
            .map_err(|_| Refusal::NoShop)?;
        let nh = *self.by_id.get(&open.npc).ok_or(Refusal::NoShop)?;
        let now = self.tick.0;
        let content = self.content.clone();
        let def = content.catalog.shop(open.shop).ok_or(Refusal::NoShop)?;
        if let Ok(s) = self.world.query_one_mut::<&mut ShopComp>(nh) {
            s.restock(def, now);
        }
        Ok((handle, nh, open.shop))
    }

    /// Buys `qty` pieces of the shop line at `index`.
    pub(crate) fn buy(
        &mut self,
        player: PlayerId,
        index: u16,
        qty: u32,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let (handle, nh, shop) = self.open_of(player)?;
        if qty == 0 {
            return Err(Refusal::NotTraded);
        }
        let content = self.content.clone();
        let cat = &content.catalog;
        let def = cat.shop(shop).ok_or(Refusal::NoShop)?;
        let stock = def
            .stock
            .get(usize::from(index))
            .copied()
            .ok_or(Refusal::NotTraded)?;
        let left = self
            .world
            .get::<&ShopComp>(nh)
            .ok()
            .and_then(|s| s.lines.get(usize::from(index)).map(|l| l.left))
            .ok_or(Refusal::NotTraded)?;
        if left < qty {
            return Err(Refusal::OutOfStock);
        }
        let idef = cat.item(stock.item).ok_or(Refusal::NotTraded)?;
        if idef.value == 0 {
            return Err(Refusal::NotTraded);
        }
        let unit = defs::buy_price(
            idef.value,
            cat.formulas().economy.sell_permille,
            def.price_permille,
        );
        let total = unit.saturating_mul(u64::from(qty));
        // Equipment is one piece at a time: each has its own identity.
        let equipment = idef.equip.is_some();
        if equipment && qty != 1 {
            return Err(Refusal::NotTraded);
        }
        // An item with identity needs a reserved id. Checked before paying,
        // so a zone out of ids never takes the coins.
        if equipment && self.item_ids.as_ref().is_none_or(|b| b.remaining() == 0) {
            out.need_item_ids = true;
            return Err(Refusal::OutOfStock);
        }
        let name = TextRef::new(idef.name);
        // Pay first, in a copy, so a full backpack changes nothing.
        let mut data = self
            .world
            .get::<&PlayerComp>(handle)
            .map(|p| p.data.clone())
            .map_err(|_| Refusal::NoShop)?;
        if data.spend_coins(CoinFlow::ShopPurchase, total).is_err() {
            self.short_of_coins(out, player, total, data.coins);
            return Err(Refusal::NotEnoughCoins);
        }
        if !equipment {
            data.give(cat, stock.item, qty)
                .map_err(|_| Refusal::InventoryFull)?;
        } else if !data.slots.contains(&Slot::Empty) {
            return Err(Refusal::InventoryFull);
        }
        if let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(handle) {
            p.data = data;
            p.dirty_bag = true;
        }
        if equipment {
            crate::fight::give_instance(self, handle, stock.item, out);
        }
        if let Ok(s) = self.world.query_one_mut::<&mut ShopComp>(nh)
            && let Some(line) = s.lines.get_mut(usize::from(index))
            && line.left != UNLIMITED
        {
            line.left -= qty;
        }
        self.mark_shop(handle);
        self.tell(
            out,
            player,
            "system.bought",
            smallvec::smallvec![
                Arg::Ref(Box::new(name)),
                Arg::Int(i64::from(qty)),
                Arg::Int(i64::try_from(total).unwrap_or(i64::MAX))
            ],
        );
        Ok(())
    }

    /// Sells `qty` of a backpack slot to the open shop.
    pub(crate) fn sell(
        &mut self,
        player: PlayerId,
        slot: u16,
        qty: u32,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let (handle, _, shop) = self.open_of(player)?;
        if qty == 0 {
            return Err(Refusal::NotTraded);
        }
        let content = self.content.clone();
        let cat = &content.catalog;
        let def = cat.shop(shop).ok_or(Refusal::NoShop)?;
        let pays = cat.formulas().economy.buy_permille;
        let (item, held) = {
            let p = self
                .world
                .get::<&PlayerComp>(handle)
                .map_err(|_| Refusal::NoShop)?;
            match p.data.slots.get(usize::from(slot)) {
                Some(Slot::Stack { item, qty: have }) if *have >= qty => (*item, false),
                Some(Slot::Held(item)) if qty == 1 => (*item, true),
                _ => return Err(Refusal::InvalidSlot),
            }
        };
        let idef = cat.item(item).ok_or(Refusal::NotTraded)?;
        // A quest item is not merchandise, and neither is a worthless one.
        if idef.value == 0 || idef.kind == base::ItemKind::Quest {
            return Err(Refusal::NotTraded);
        }
        let mut unit = defs::sell_price(idef.value, pays, def.price_permille);
        if held {
            // A worn piece is worth what is left of it, and its rarity
            // multiplies what a piece of the item's own rarity is worth
            // no NPC buys one rarer than `npc_buy_max`.
            let (cur, max, rarity) = self
                .world
                .get::<&Gear>(handle)
                .ok()
                .and_then(|g| {
                    let i = g.bag.iter().find(|(s, _)| *s == slot)?;
                    Some((i.1.durability, i.1.max_durability, i.1.rarity))
                })
                .unwrap_or((0, 0, idef.rarity));
            let table = &cat.formulas().economy.rarity_value_permille;
            let (piece, own) = (
                defs::EconomyFormulas::by_rarity(table, rarity),
                defs::EconomyFormulas::by_rarity(table, idef.rarity).max(1),
            );
            if piece == 0 || rarity > cat.formulas().economy.npc_buy_max {
                return Err(Refusal::NotTraded);
            }
            unit = unit.saturating_mul(u64::from(piece)) / u64::from(own);
            if max > 0 {
                unit = unit.saturating_mul(u64::from(cur)) / u64::from(max);
            }
        }
        let total = unit.saturating_mul(u64::from(qty));
        let name = TextRef::new(idef.name);
        // What the shop still pays this player in its purse period.
        if let Some(purse) = def.purse {
            let now = self.now_unix;
            let left = self
                .world
                .get::<&PlayerComp>(handle)
                .map(|p| purse_left(&p.data.shop_sales, &def.id, purse, now))
                .map_err(|_| Refusal::NoShop)?;
            if total > left.0 {
                let args: SmallVec<[Arg; 2]> = smallvec::smallvec![
                    Arg::Int(i64::try_from(left.0).unwrap_or(i64::MAX)),
                    Arg::Int(i64::from(left.1.div_ceil(60).max(1)))
                ];
                self.tell(out, player, "system.shop_purse_empty", args);
                return Err(Refusal::NotTraded);
            }
            if let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(handle) {
                spend_purse(&mut p.data.shop_sales, &def.id, purse, now, total);
            }
        }
        if held {
            let Ok((p, gear)) = self.world.query_one_mut::<(&mut PlayerComp, &mut Gear)>(handle) else {
                return Err(Refusal::NoShop);
            };
            if gear.take_bag(slot).is_none() {
                return Err(Refusal::InvalidSlot);
            }
            if let Some(s) = p.data.slots.get_mut(usize::from(slot)) {
                *s = Slot::Empty;
            }
            p.data.gain_coins(CoinFlow::ShopSale, total);
            p.dirty_bag = true;
            p.dirty_stats = true;
        } else {
            let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(handle) else {
                return Err(Refusal::NoShop);
            };
            p.data.take(item, qty).map_err(|_| Refusal::InvalidSlot)?;
            p.data.gain_coins(CoinFlow::ShopSale, total);
            p.dirty_bag = true;
        }
        self.mark_shop(handle);
        self.tell(
            out,
            player,
            "system.sold",
            smallvec::smallvec![
                Arg::Ref(Box::new(name)),
                Arg::Int(i64::from(qty)),
                Arg::Int(i64::try_from(total).unwrap_or(i64::MAX))
            ],
        );
        Ok(())
    }

    /// Repairs the equipment of a backpack slot at the open shop. An NPC
    /// repair costs coins and a little maximum durability.
    pub(crate) fn repair(
        &mut self,
        player: PlayerId,
        slot: u16,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let (handle, _, shop) = self.open_of(player)?;
        let content = self.content.clone();
        let cat = &content.catalog;
        let def = cat.shop(shop).ok_or(Refusal::NoShop)?;
        if !def.repairs {
            return Err(Refusal::NotTraded);
        }
        let economy = &cat.formulas().economy;
        let (item, cur, max) = self
            .world
            .get::<&Gear>(handle)
            .ok()
            .and_then(|g| {
                let (_, i) = g.bag.iter().find(|(s, _)| *s == slot)?;
                Some((i.item, i.durability, i.max_durability))
            })
            .ok_or(Refusal::InvalidSlot)?;
        if max == 0 || cur >= max {
            return Err(Refusal::NotTraded);
        }
        let value = cat.item(item).map_or(0, |d| d.value);
        let price = defs::repair_price(value, max - cur, max, economy.repair_permille, def.price_permille);
        let name = cat.item(item).map(|d| d.name);
        {
            let p = self
                .world
                .query_one_mut::<&mut PlayerComp>(handle)
                .map_err(|_| Refusal::NoShop)?;
            if p.data.spend_coins(CoinFlow::Repair, price).is_err() {
                let have = p.data.coins;
                self.short_of_coins(out, player, price, have);
                return Err(Refusal::NotEnoughCoins);
            }
            p.dirty_bag = true;
            p.dirty_stats = true;
        }
        if let Ok(gear) = self.world.query_one_mut::<&mut Gear>(handle)
            && let Some(inst) = gear.bag_item(slot)
        {
            inst.repair(economy.repair_max_loss_permille);
        }
        self.mark_shop(handle);
        let args: SmallVec<[Arg; 2]> = smallvec::smallvec![
            name.map(|n| Arg::Ref(Box::new(TextRef::new(n))))
                .unwrap_or(Arg::Int(0)),
            Arg::Int(i64::try_from(price).unwrap_or(i64::MAX))
        ];
        self.tell(out, player, "system.repaired", args);
        Ok(())
    }

    /// Says how many coins the action costs and how many the
    /// player has, so "not enough" is never a guess.
    pub(crate) fn short_of_coins(&self, out: &mut TickOutput, player: PlayerId, price: u64, have: u64) {
        let int = |v: u64| Arg::Int(i64::try_from(v).unwrap_or(i64::MAX));
        self.tell(
            out,
            player,
            "system.not_enough_coins",
            smallvec::smallvec![int(price), int(have)],
        );
    }

    /// A production class mends a backpack piece with the main
    /// material of the piece's recipe (the one asked in the largest
    /// quantity), in proportion to the wear, anywhere. It needs the
    /// recipe's proficiency points, not the recipe itself; like any
    /// artificer it keeps the maximum and grows the proficiency
    pub(crate) fn self_repair(
        &mut self,
        player: PlayerId,
        slot: u16,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let handle = *self.by_player.get(&player).ok_or(Refusal::InvalidSlot)?;
        let content = self.content.clone();
        let cat = &content.catalog;
        let (item, cur, max) = self
            .world
            .get::<&Gear>(handle)
            .ok()
            .and_then(|g| {
                let (_, i) = g.bag.iter().find(|(s, _)| *s == slot)?;
                Some((i.item, i.durability, i.max_durability))
            })
            .ok_or(Refusal::InvalidSlot)?;
        if max == 0 || cur >= max {
            return Err(Refusal::NotTraded);
        }
        let item_name = cat.item(item).map(|d| d.name);
        let named = |t: Option<base::TextId>| t.map_or(Arg::Int(0), |n| Arg::Ref(Box::new(TextRef::new(n))));
        let (production, points_of) = {
            let p = self
                .world
                .get::<&PlayerComp>(handle)
                .map_err(|_| Refusal::InvalidSlot)?;
            let sheet = p.growth.sheet.clone();
            (crate::craft::is_production(&sheet, cat), sheet)
        };
        if !production {
            self.tell(out, player, "system.self_repair_class_only", SmallVec::new());
            return Err(Refusal::RequirementsNotMet);
        }
        let Some(recipe) = cat.recipes().map(|(_, r)| r).find(|r| r.result == item) else {
            self.tell(
                out,
                player,
                "system.self_repair_no_recipe",
                smallvec::smallvec![named(item_name)],
            );
            return Err(Refusal::NotTraded);
        };
        if points_of.proficiency(recipe.proficiency) < recipe.min_points {
            let prof = cat.proficiency(recipe.proficiency).map(|d| d.name);
            self.tell(
                out,
                player,
                "system.self_repair_points",
                smallvec::smallvec![named(prof), Arg::Int(i64::from(recipe.min_points))],
            );
            return Err(Refusal::RequirementsNotMet);
        }
        let Some(main) = recipe.materials.iter().max_by_key(|m| m.qty) else {
            return Err(Refusal::NotTraded);
        };
        // The share of the recipe's bars that the wear is worth, at least one.
        let need = u32::try_from((u64::from(main.qty) * u64::from(max - cur)).div_ceil(u64::from(max)))
            .unwrap_or(u32::MAX)
            .max(1);
        let material_name = cat.item(main.item).map(|d| d.name);
        {
            let p = self
                .world
                .query_one_mut::<&mut PlayerComp>(handle)
                .map_err(|_| Refusal::InvalidSlot)?;
            let have = p.data.count(main.item);
            if p.data.take(main.item, need).is_err() {
                let mut args: SmallVec<[Arg; 2]> =
                    smallvec::smallvec![named(material_name), Arg::Int(i64::from(need))];
                args.push(Arg::Int(i64::try_from(have).unwrap_or(i64::MAX)));
                self.tell(out, player, "system.self_repair_material", args);
                return Err(Refusal::RequirementsNotMet);
            }
            p.dirty_bag = true;
            p.dirty_stats = true;
        }
        if let Ok(gear) = self.world.query_one_mut::<&mut Gear>(handle)
            && let Some(inst) = gear.bag_item(slot)
        {
            inst.repair(0);
        }
        self.reward_work(handle, player, recipe.proficiency, 0, 0, out);
        let mut args: SmallVec<[Arg; 2]> = smallvec::smallvec![named(item_name), Arg::Int(i64::from(need))];
        args.push(named(material_name));
        self.tell(out, player, "system.self_repaired", args);
        Ok(())
    }

    /// The open shop appraises a backpack piece for coins: always reveals
    pub(crate) fn shop_appraise(
        &mut self,
        player: PlayerId,
        slot: u16,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let (handle, _, shop) = self.open_of(player)?;
        let content = self.content.clone();
        let cat = &content.catalog;
        let def = cat.shop(shop).ok_or(Refusal::NoShop)?;
        if !def.appraises {
            return Err(Refusal::NotTraded);
        }
        let (item, appraised) = self
            .world
            .get::<&Gear>(handle)
            .ok()
            .and_then(|g| {
                let (_, i) = g.bag.iter().find(|(s, _)| *s == slot)?;
                Some((i.item, i.appraised))
            })
            .ok_or(Refusal::InvalidSlot)?;
        if appraised {
            return Err(Refusal::NotReady);
        }
        let price = appraise_price(cat, item, def);
        let name = cat.item(item).map(|d| d.name);
        {
            let p = self
                .world
                .query_one_mut::<&mut PlayerComp>(handle)
                .map_err(|_| Refusal::NoShop)?;
            if p.data.spend_coins(CoinFlow::Appraise, price).is_err() {
                let have = p.data.coins;
                self.short_of_coins(out, player, price, have);
                return Err(Refusal::NotEnoughCoins);
            }
            p.dirty_bag = true;
        }
        if let Ok(gear) = self.world.query_one_mut::<&mut Gear>(handle)
            && let Some(inst) = gear.bag_item(slot)
        {
            inst.appraised = true;
        }
        self.mark_shop(handle);
        let args: SmallVec<[Arg; 2]> = smallvec::smallvec![
            name.map(|n| Arg::Ref(Box::new(TextRef::new(n))))
                .unwrap_or(Arg::Int(0)),
            Arg::Int(i64::try_from(price).unwrap_or(i64::MAX))
        ];
        self.tell(out, player, "system.appraised_paid", args);
        Ok(())
    }

    fn mark_shop(&mut self, handle: hecs::Entity) {
        if let Ok(open) = self.world.query_one_mut::<&mut ShopOpen>(handle) {
            open.dirty = true;
        }
    }

    /// The NPCs of the zone that keep a shop, for players who just
    /// arrived or whose content was reloaded (phase 7).
    pub(crate) fn flush_shop_npcs(&mut self, out: &mut TickOutput) {
        let players: Vec<(PlayerId, hecs::Entity)> = self
            .by_player
            .iter()
            .filter(|(_, h)| self.world.get::<&PlayerComp>(**h).is_ok_and(|p| p.dirty_shops))
            .map(|(p, h)| (*p, *h))
            .collect();
        if players.is_empty() {
            return;
        }
        let cat = &self.content.catalog;
        let keepers: Vec<EntityId> = self
            .by_id
            .iter()
            .filter(|(_, h)| {
                self.world
                    .get::<&NpcComp>(**h)
                    .ok()
                    .and_then(|n| cat.npc(n.npc).map(|d| d.shop.is_some()))
                    .unwrap_or(false)
            })
            .map(|(id, _)| *id)
            .collect();
        for (player, handle) in players {
            if let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(handle) {
                p.dirty_shops = false;
            }
            out.shop_npcs.push((player, keepers.clone()));
        }
    }

    /// `Shop` for every player whose open shop changed this tick.
    pub(crate) fn flush_shops(&mut self, out: &mut TickOutput) {
        let players: Vec<(PlayerId, hecs::Entity)> = self
            .by_player
            .iter()
            .filter(|(_, h)| self.world.get::<&ShopOpen>(**h).is_ok_and(|o| o.dirty))
            .map(|(p, h)| (*p, *h))
            .collect();
        for (player, handle) in players {
            if let Ok(open) = self.world.query_one_mut::<&mut ShopOpen>(handle) {
                open.dirty = false;
            }
            if let Some(view) = self.shop_view(handle) {
                out.shops.push((player, Some(view)));
            }
        }
    }

    fn shop_view(&self, handle: hecs::Entity) -> Option<ShopView> {
        let open = self.world.get::<&ShopOpen>(handle).ok()?;
        let cat = &self.content.catalog;
        let def = cat.shop(open.shop)?;
        let nh = *self.by_id.get(&open.npc)?;
        let state = self.world.get::<&ShopComp>(nh).ok()?;
        let economy = &cat.formulas().economy;
        let stock = def
            .stock
            .iter()
            .zip(state.lines.iter())
            .map(|(s, l)| ShopLineView {
                item: s.item,
                qty: (l.left != UNLIMITED).then_some(l.left),
                price: price_of(cat, s.item, def, economy.sell_permille),
            })
            .collect();
        let purse = def.purse.and_then(|purse| {
            let p = self.world.get::<&PlayerComp>(handle).ok()?;
            Some(purse_left(&p.data.shop_sales, &def.id, purse, self.now_unix))
        });
        Some(ShopView {
            npc: open.npc,
            stock,
            pays_permille: economy.buy_permille,
            repair_permille: if def.repairs { economy.repair_permille } else { 0 },
            price_permille: def.price_permille,
            appraise_permille: if def.appraises {
                economy.appraise_permille
            } else {
                0
            },
            purse,
        })
    }
}

/// What an appraisal at this shop costs.
pub(crate) fn appraise_price(cat: &ValidatedCatalog, item: ItemId, def: &ShopDef) -> u64 {
    let value = cat.item(item).map_or(0, |d| d.value);
    let share = value.saturating_mul(u64::from(cat.formulas().economy.appraise_permille)) / 1000;
    (share.saturating_mul(u64::from(def.price_permille)) / 1000).max(1)
}

fn purse_secs(purse: Purse) -> u64 {
    u64::from(purse.refill_secs)
}

/// Coins the shop still pays and seconds until its purse refills.
pub(crate) fn purse_left(sales: &[(String, u64, u64)], shop: &str, purse: Purse, now: u64) -> (u64, u32) {
    let period = purse_secs(purse);
    match sales.iter().find(|(s, _, _)| s == shop) {
        Some((_, paid, since)) if now < since.saturating_add(period) => (
            purse.coins.saturating_sub(*paid),
            u32::try_from(since.saturating_add(period) - now).unwrap_or(u32::MAX),
        ),
        _ => (purse.coins, u32::try_from(period).unwrap_or(u32::MAX)),
    }
}

/// Counts `coins` paid against the purse, opening a new period when the
/// last one ended. Periods that ended are dropped.
pub(crate) fn spend_purse(
    sales: &mut Vec<(String, u64, u64)>,
    shop: &str,
    purse: Purse,
    now: u64,
    coins: u64,
) {
    let period = purse_secs(purse);
    match sales.iter_mut().find(|(s, _, _)| s == shop) {
        Some(e) if now < e.2.saturating_add(period) => e.1 = e.1.saturating_add(coins),
        Some(e) => *e = (shop.to_owned(), coins, now),
        None => sales.push((shop.to_owned(), coins, now)),
    }
}

fn price_of(cat: &ValidatedCatalog, item: ItemId, def: &ShopDef, sell_permille: u32) -> u64 {
    let value = cat.item(item).map_or(0, |d| d.value);
    defs::buy_price(value, sell_permille, def.price_permille)
}
