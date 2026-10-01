//! Custody (`escrow`, [ ]
//! rule 4): coins, stacks and pieces leave the sheet and wait here until
//! the database writes the new sheet and the parcel in one transaction.
//! A refusal gives everything back; a delivery puts a parcel taken out of
//! the database into the backpack.
//!
//! The pieces stay [`ItemInstance`]s while they wait (moved, never
//! copied); the parcel the binary writes is their saved form, with the
//! same id, crafter, rarity, durability and hidden bonuses.

use crate::Zone;
use crate::gear::{Gear, ItemInstance};
use crate::player::{PlayerData, Slot};
use crate::types::{
    CharacterSheet, DeliveredOut, EscrowOut, EscrowParcel, ItemPlace, Refusal, SavedStack, TickOutput,
};
use base::{CharacterId, CharacterName, ItemId, ItemKind, PlayerId};
use serde::{Deserialize, Serialize};

/// Deposits waiting for the database, sorted by ticket.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Escrows {
    next: u64,
    pending: Vec<Pending>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Pending {
    ticket: u64,
    player: PlayerId,
    from: CharacterId,
    coins: u64,
    stacks: Vec<(ItemId, u32)>,
    items: Vec<ItemInstance>,
}

impl Escrows {
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.pending.len()
    }
}

impl Zone {
    /// Takes `items` (`(slot, qty)`) and `coins` out of the player into
    /// custody. `Err` changes nothing.
    pub(crate) fn escrow_deposit(
        &mut self,
        player: PlayerId,
        from: CharacterId,
        items: &[(u16, u32)],
        coins: u64,
        to: &CharacterName,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        if items.is_empty() && coins == 0 {
            return Err(Refusal::InvalidSlot);
        }
        let h = *self.by_player.get(&player).ok_or(Refusal::InvalidTarget)?;
        let content = self.content.clone();
        let cat = &content.catalog;
        let (mut data, sender) = {
            let p = self
                .world
                .get::<&crate::zone::PlayerComp>(h)
                .map_err(|_| Refusal::InvalidTarget)?;
            (p.data.clone(), p.sheet.name.clone())
        };
        // Nobody sends to themselves: custody is not a second backpack.
        if &sender == to {
            return Err(Refusal::InvalidTarget);
        }
        data.coins = data.coins.checked_sub(coins).ok_or(Refusal::NotEnoughCoins)?;
        let mut stacks = Vec::new();
        let mut held = Vec::new();
        {
            let gear = self.world.get::<&Gear>(h).map_err(|_| Refusal::InvalidSlot)?;
            for (n, &(slot, qty)) in items.iter().enumerate() {
                if qty == 0 || items.iter().take(n).any(|(s, _)| *s == slot) {
                    return Err(Refusal::InvalidSlot);
                }
                let s = data
                    .slots
                    .get_mut(usize::from(slot))
                    .ok_or(Refusal::InvalidSlot)?;
                let item = match s {
                    Slot::Stack { item, qty: have } if *have >= qty => *item,
                    Slot::Held(item) if qty == 1 && gear.bag.iter().any(|(b, _)| *b == slot) => *item,
                    _ => return Err(Refusal::InvalidSlot),
                };
                // A quest item stays with its quest, as in a trade.
                if cat.item(item).is_none_or(|d| d.kind == ItemKind::Quest) {
                    return Err(Refusal::InvalidSlot);
                }
                match s {
                    Slot::Stack { qty: have, .. } => {
                        *have -= qty;
                        if *have == 0 {
                            *s = Slot::Empty;
                        }
                        stacks.push((item, qty));
                    }
                    _ => {
                        *s = Slot::Empty;
                        held.push(slot);
                    }
                }
            }
        }
        // Past this point nothing fails: every slot was checked above.
        let ticket = self.escrows.next;
        self.escrows.next += 1;
        let Ok((p, gear)) = self
            .world
            .query_one_mut::<(&mut crate::zone::PlayerComp, &mut Gear)>(h)
        else {
            return Err(Refusal::InvalidTarget);
        };
        let pieces: Vec<ItemInstance> = held.iter().filter_map(|&slot| gear.take_bag(slot)).collect();
        p.data = data;
        p.dirty_bag = true;
        let parcel = EscrowParcel {
            coins,
            stacks: stacks
                .iter()
                .filter_map(|&(item, qty)| {
                    Some(SavedStack {
                        item: cat.item(item)?.id.clone(),
                        qty,
                    })
                })
                .collect(),
            items: pieces
                .iter()
                .filter_map(|i| i.save(cat, ItemPlace::Backpack(0)))
                .collect(),
        };
        self.escrows.pending.push(Pending {
            ticket,
            player,
            from,
            coins,
            stacks,
            items: pieces,
        });
        let (sheet, pos) = self.export_player(h).ok_or(Refusal::InvalidTarget)?;
        out.escrows.push(EscrowOut {
            ticket,
            player,
            from,
            to: to.clone(),
            parcel,
            sheet,
            pos,
        });
        Ok(())
    }

    /// The database wrote the deposit (`ok`) or refused it: then the
    /// parcel goes back to the player, or to `escrow_lost` when it cannot.
    pub(crate) fn escrow_done(&mut self, ticket: u64, ok: bool, out: &mut TickOutput) {
        let Some(i) = self.escrows.pending.iter().position(|p| p.ticket == ticket) else {
            return;
        };
        let pending = self.escrows.pending.remove(i);
        let player = pending.player;
        if ok {
            self.tell(out, player, "system.escrow_sent", smallvec::smallvec![]);
            return;
        }
        let content = self.content.clone();
        let cat = &content.catalog;
        match self.give_back(pending) {
            Ok(()) => self.tell(out, player, "system.escrow_returned", smallvec::smallvec![]),
            Err(p) => {
                let parcel = EscrowParcel {
                    coins: p.coins,
                    stacks: p
                        .stacks
                        .iter()
                        .filter_map(|&(item, qty)| {
                            Some(SavedStack {
                                item: cat.item(item)?.id.clone(),
                                qty,
                            })
                        })
                        .collect(),
                    items: p
                        .items
                        .iter()
                        .filter_map(|i| i.save(cat, ItemPlace::Backpack(0)))
                        .collect(),
                };
                out.escrow_lost.push((ticket, p.from, parcel));
            }
        }
    }

    /// Everything back in the backpack, or nothing (`Err` with the deposit).
    fn give_back(&mut self, p: Pending) -> Result<(), Pending> {
        let Some(&h) = self.by_player.get(&p.player) else {
            return Err(p);
        };
        let content = self.content.clone();
        let cat = &content.catalog;
        let Ok((pc, gear)) = self
            .world
            .query_one_mut::<(&mut crate::zone::PlayerComp, &mut Gear)>(h)
        else {
            return Err(p);
        };
        let mut data = pc.data.clone();
        let Some(slots) = fill(&mut data, cat, p.coins, &p.stacks, p.items.iter().map(|i| i.item)) else {
            return Err(p);
        };
        gear.bag.extend(slots.into_iter().zip(p.items));
        pc.data = data;
        pc.dirty_bag = true;
        Ok(())
    }

    /// A parcel out of the database into the backpack, all or nothing.
    pub(crate) fn deliver(
        &mut self,
        player: PlayerId,
        escrow: u64,
        parcel: &EscrowParcel,
        out: &mut TickOutput,
    ) {
        let result = self.deliver_into(player, parcel);
        match &result {
            Ok(_) => self.tell(out, player, "system.escrow_received", smallvec::smallvec![]),
            Err(_) if self.by_player.contains_key(&player) => {
                self.tell(out, player, "system.escrow_no_room", smallvec::smallvec![])
            }
            Err(_) => {}
        }
        out.delivered.push(DeliveredOut {
            player,
            escrow,
            result: result.map_err(|()| parcel.clone()),
        });
    }

    fn deliver_into(
        &mut self,
        player: PlayerId,
        parcel: &EscrowParcel,
    ) -> Result<(CharacterSheet, base::TilePos), ()> {
        let h = *self.by_player.get(&player).ok_or(())?;
        let content = self.content.clone();
        let cat = &content.catalog;
        // Pieces the running content lost are kept as saved,
        // like on entering; they take no slot.
        let incoming = Gear::import(&parcel.items, cat);
        let mut stacks = Vec::new();
        let mut unknown = Vec::new();
        for s in &parcel.stacks {
            match cat.item_by_name(&s.item) {
                Some((item, _)) if s.qty > 0 => stacks.push((item, s.qty)),
                Some(_) => {}
                None => unknown.push(s.clone()),
            }
        }
        {
            let (pc, gear) = self
                .world
                .query_one_mut::<(&mut crate::zone::PlayerComp, &mut Gear)>(h)
                .map_err(|_| ())?;
            let mut data = pc.data.clone();
            let pieces: Vec<ItemInstance> = incoming.bag.into_iter().map(|(_, i)| i).collect();
            let slots = fill(
                &mut data,
                cat,
                parcel.coins,
                &stacks,
                pieces.iter().map(|i| i.item),
            )
            .ok_or(())?;
            for s in unknown {
                let free = data.slots.iter().position(|x| *x == Slot::Empty).ok_or(())?;
                if let Some(x) = data.slots.get_mut(free) {
                    *x = Slot::Unknown(s);
                }
            }
            gear.bag.extend(slots.into_iter().zip(pieces));
            gear.kept.extend(incoming.kept);
            pc.data = data;
            pc.dirty_bag = true;
        }
        self.export_player(h).ok_or(())
    }

    /// Sheet (with HP) and tile of a player still in the zone.
    fn export_player(&self, h: hecs::Entity) -> Option<(CharacterSheet, base::TilePos)> {
        let pos = self.world.get::<&crate::zone::Body>(h).ok()?.pos;
        Some((self.export_handle(h)?, pos))
    }
}

/// Coins, stacks and one free slot per piece, on `data`. Returns the slots
/// taken for the pieces, or `None` (then `data` is half changed: callers
/// work on a copy).
fn fill(
    data: &mut PlayerData,
    cat: &defs::ValidatedCatalog,
    coins: u64,
    stacks: &[(ItemId, u32)],
    pieces: impl Iterator<Item = ItemId>,
) -> Option<Vec<u16>> {
    data.coins = data.coins.saturating_add(coins);
    for &(item, qty) in stacks {
        data.give(cat, item, qty).ok()?;
    }
    let mut taken = Vec::new();
    for item in pieces {
        let free = data.slots.iter().position(|s| *s == Slot::Empty)?;
        if let Some(s) = data.slots.get_mut(free) {
            *s = Slot::Held(item);
        }
        taken.push(u16::try_from(free).ok()?);
    }
    Some(taken)
}
