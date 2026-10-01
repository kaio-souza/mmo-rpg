//! Groups and direct trade (phase 9,
//! and).
//!
//! Both live in [`Social`], one value on the zone, keyed by `PlayerId`.
//! A trade keeps **references** to what each side offers (backpack slot,
//! quantity and the identity it saw there), never the items: nothing leaves
//! a backpack until both sides confirmed, and then the swap is checked on
//! copies and applied inside one command, so an item is never in two
//! places and a drop in the middle cannot lose or duplicate one
//!
//! Group XP is split in [`crate::fight`]; this module only answers who is
//! in whose group.

use crate::Zone;
use crate::fight::Fighter;
use crate::gear::Gear;
use crate::player::{PlayerData, Slot};
use crate::types::{InviteKind, InviteOut, PartyView, Refusal, TickOutput, TradeView};
use crate::zone::{Body, PlayerComp};
use base::{Arg, EntityId, ItemId, ItemKind, PlayerId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Members of a group, leader included.
pub const MAX_PARTY: usize = 6;
/// Farthest two players may be to start and keep a trade, in tiles.
pub const TRADE_RADIUS: u32 = 5;
/// An unanswered request, or an offer nobody touched, lasts this long
/// (30 s at 20 Hz).
pub const TRADE_TICKS: u64 = 600;
/// An unanswered group invite lasts this long.
pub const INVITE_TICKS: u64 = 600;
/// How near a member must be to the kill to share its XP, in tiles.
pub const SHARE_RADIUS: u32 = 40;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Party {
    pub id: u32,
    pub leader: PlayerId,
    /// Leader first, then by join order.
    pub members: Vec<PlayerId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Invite {
    pub from: PlayerId,
    pub to: PlayerId,
    pub until: u64,
}

/// What one side put on the table, as it saw it in the backpack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Offer {
    pub slot: u16,
    pub qty: u32,
    pub item: ItemId,
    /// Identity of an item with identity; `None` for a stack.
    pub instance: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Side {
    pub player: PlayerId,
    pub offers: Vec<Offer>,
    pub coins: u64,
    pub confirmed: bool,
}

impl Side {
    fn new(player: PlayerId) -> Side {
        Side {
            player,
            offers: Vec::new(),
            coins: 0,
            confirmed: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Trade {
    /// The one who asked first.
    pub a: Side,
    pub b: Side,
    /// Both asked: offers may be set.
    pub open: bool,
    pub until: u64,
}

impl Trade {
    fn has(&self, p: PlayerId) -> bool {
        self.a.player == p || self.b.player == p
    }

    fn other(&self, p: PlayerId) -> PlayerId {
        if self.a.player == p {
            self.b.player
        } else {
            self.a.player
        }
    }

    fn side_mut(&mut self, p: PlayerId) -> Option<&mut Side> {
        if self.a.player == p {
            Some(&mut self.a)
        } else if self.b.player == p {
            Some(&mut self.b)
        } else {
            None
        }
    }
}

/// Every group, invite and trade of the zone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Social {
    pub next_party: u32,
    pub parties: Vec<Party>,
    pub invites: Vec<Invite>,
    pub trades: Vec<Trade>,
    /// Players whose group or trade view has to be sent again. Drained by
    /// the tick, so it is empty between ticks.
    pub dirty: BTreeSet<PlayerId>,
}

impl Social {
    pub fn party_of(&self, p: PlayerId) -> Option<&Party> {
        self.parties.iter().find(|g| g.members.contains(&p))
    }

    fn trade_index(&self, p: PlayerId) -> Option<usize> {
        self.trades.iter().position(|t| t.has(p))
    }

    /// Takes `p` out of its group; the group ends below two members.
    fn leave(&mut self, p: PlayerId) {
        let Some(i) = self.parties.iter().position(|g| g.members.contains(&p)) else {
            return;
        };
        let Some(group) = self.parties.get_mut(i) else {
            return;
        };
        self.dirty.extend(group.members.iter().copied());
        group.members.retain(|m| *m != p);
        if group.leader == p
            && let Some(&next) = group.members.first()
        {
            group.leader = next;
        }
        if group.members.len() < 2 {
            self.parties.remove(i);
        }
    }
}

impl Zone {
    /// The player behind an entity id, with its handle.
    pub(crate) fn player_at(&self, id: EntityId) -> Option<(PlayerId, hecs::Entity)> {
        let &h = self.by_id.get(&id)?;
        let p = self.world.get::<&PlayerComp>(h).ok()?.player;
        Some((p, h))
    }

    fn entity_of_player(&self, p: PlayerId) -> Option<EntityId> {
        let &h = self.by_player.get(&p)?;
        self.world.get::<&Body>(h).ok().map(|b| b.id)
    }

    pub(crate) fn pos_of_handle(&self, h: hecs::Entity) -> Option<base::TilePos> {
        self.world.get::<&Body>(h).ok().map(|b| b.pos)
    }

    /// The group a player is in (read by the XP split).
    pub(crate) fn party_members(&self, p: PlayerId) -> Option<&[PlayerId]> {
        self.social.party_of(p).map(|g| g.members.as_slice())
    }

    // Group.

    pub(crate) fn party_invite(
        &mut self,
        player: PlayerId,
        target: EntityId,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let (to, _) = self.player_at(target).ok_or(Refusal::InvalidTarget)?;
        if to == player {
            return Err(Refusal::InvalidTarget);
        }
        let me = self.entity_of_player(player).ok_or(Refusal::NoParty)?;
        if let Some(g) = self.social.party_of(player) {
            if g.leader != player {
                return Err(Refusal::NoParty);
            }
            if g.members.len() >= MAX_PARTY {
                return Err(Refusal::PartyFull);
            }
        }
        if self.social.party_of(to).is_some() {
            return Err(Refusal::InvalidTarget);
        }
        let until = self.tick.0 + INVITE_TICKS;
        self.social.invites.retain(|i| !(i.from == player && i.to == to));
        self.social.invites.push(Invite {
            from: player,
            to,
            until,
        });
        out.invites.push(InviteOut {
            to,
            from: me,
            kind: InviteKind::Party,
        });
        Ok(())
    }

    pub(crate) fn answer_invite(
        &mut self,
        player: PlayerId,
        accept: bool,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let i = self
            .social
            .invites
            .iter()
            .rposition(|i| i.to == player)
            .ok_or(Refusal::NoParty)?;
        let invite = self.social.invites.remove(i);
        if !accept {
            return Ok(());
        }
        // The invite is spent whatever happens next.
        self.social.invites.retain(|i| i.to != player);
        if !self.by_player.contains_key(&invite.from) || self.social.party_of(player).is_some() {
            return Err(Refusal::NoParty);
        }
        let id = match self.social.party_of(invite.from) {
            Some(g) if g.leader != invite.from => return Err(Refusal::NoParty),
            Some(g) if g.members.len() >= MAX_PARTY => return Err(Refusal::PartyFull),
            Some(g) => g.id,
            None => {
                self.social.next_party += 1;
                let id = self.social.next_party;
                self.social.parties.push(Party {
                    id,
                    leader: invite.from,
                    members: vec![invite.from],
                });
                id
            }
        };
        if let Some(g) = self.social.parties.iter_mut().find(|g| g.id == id) {
            g.members.push(player);
            self.social.dirty.extend(g.members.iter().copied());
        }
        let name = self.name_of(player);
        let others: Vec<PlayerId> = self
            .social
            .party_of(player)
            .map(|g| g.members.iter().copied().filter(|m| *m != player).collect())
            .unwrap_or_default();
        for m in others {
            self.tell(
                out,
                m,
                "system.party_joined",
                smallvec::smallvec![Arg::Str(name.clone())],
            );
        }
        Ok(())
    }

    pub(crate) fn leave_party(&mut self, player: PlayerId, out: &mut TickOutput) {
        let name = self.name_of(player);
        let others: Vec<PlayerId> = self
            .social
            .party_of(player)
            .map(|g| g.members.iter().copied().filter(|m| *m != player).collect())
            .unwrap_or_default();
        self.social.leave(player);
        self.social.dirty.insert(player);
        for m in others {
            self.tell(
                out,
                m,
                "system.party_left",
                smallvec::smallvec![Arg::Str(name.clone())],
            );
        }
    }

    pub(crate) fn party_kick(
        &mut self,
        player: PlayerId,
        target: EntityId,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let (who, _) = self.player_at(target).ok_or(Refusal::InvalidTarget)?;
        match self.social.party_of(player) {
            Some(g) if g.leader == player && who != player && g.members.contains(&who) => {}
            _ => return Err(Refusal::NoParty),
        }
        self.leave_party(who, out);
        Ok(())
    }

    pub(crate) fn party_lead(&mut self, player: PlayerId, target: EntityId) -> Result<(), Refusal> {
        let (who, _) = self.player_at(target).ok_or(Refusal::InvalidTarget)?;
        let Some(g) = self.social.parties.iter_mut().find(|g| g.leader == player) else {
            return Err(Refusal::NoParty);
        };
        if who == player || !g.members.contains(&who) {
            return Err(Refusal::NoParty);
        }
        g.leader = who;
        g.members.retain(|m| *m != who);
        g.members.insert(0, who);
        self.social.dirty.extend(g.members.iter().copied());
        Ok(())
    }

    pub(crate) fn name_of(&self, player: PlayerId) -> String {
        self.by_player
            .get(&player)
            .and_then(|h| self.world.get::<&PlayerComp>(*h).ok())
            .map(|p| p.sheet.name.to_string())
            .unwrap_or_default()
    }

    // Emotes.

    /// A gesture: seen by the emoter and by everyone who knows it.
    /// Fallen players cannot, and each emote has its own wait.
    pub(crate) fn emote(
        &mut self,
        player: PlayerId,
        emote: base::EmoteId,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let handle = *self.by_player.get(&player).ok_or(Refusal::InvalidTarget)?;
        if !self.standing(handle) {
            return Err(Refusal::Fallen);
        }
        let cooldown = self
            .content
            .catalog
            .emote(emote)
            .ok_or(Refusal::RequirementsNotMet)?
            .cooldown_ticks;
        let id = self.entity_of_player(player).ok_or(Refusal::InvalidTarget)?;
        let now = self.tick.0;
        let p = self
            .world
            .query_one_mut::<&mut PlayerComp>(handle)
            .map_err(|_| Refusal::InvalidTarget)?;
        if now < p.emote_ready {
            return Err(Refusal::NotReady);
        }
        p.emote_ready = now + u64::from(cooldown);
        for (&viewer, &h) in &self.by_player {
            let sees = viewer == player
                || self
                    .world
                    .get::<&PlayerComp>(h)
                    .is_ok_and(|v| v.known.binary_search(&id).is_ok());
            if sees {
                out.emotes.push((viewer, id, emote));
            }
        }
        Ok(())
    }

    // Trade.

    /// A request; the same request from the other side opens the trade.
    pub(crate) fn trade_request(
        &mut self,
        player: PlayerId,
        target: EntityId,
        out: &mut TickOutput,
    ) -> Result<(), Refusal> {
        let (other, oh) = self.player_at(target).ok_or(Refusal::InvalidTarget)?;
        let me = self.entity_of_player(player).ok_or(Refusal::InvalidTarget)?;
        let mh = *self.by_player.get(&player).ok_or(Refusal::InvalidTarget)?;
        if other == player || !self.standing(mh) || !self.standing(oh) {
            return Err(Refusal::InvalidTarget);
        }
        let (mp, op) = (
            self.pos_of_handle(mh).ok_or(Refusal::InvalidTarget)?,
            self.pos_of_handle(oh).ok_or(Refusal::InvalidTarget)?,
        );
        if mp.distance(op) > TRADE_RADIUS {
            return Err(Refusal::TooFar);
        }
        let until = self.tick.0 + TRADE_TICKS;
        // The other side already asked: this is the answer.
        if let Some(t) = self
            .social
            .trades
            .iter_mut()
            .find(|t| !t.open && t.a.player == other && t.b.player == player)
        {
            t.open = true;
            t.until = until;
            self.social.dirty.extend([player, other]);
            return Ok(());
        }
        if self.social.trade_index(player).is_some() || self.social.trade_index(other).is_some() {
            return Err(Refusal::InvalidTarget);
        }
        self.social.trades.push(Trade {
            a: Side::new(player),
            b: Side::new(other),
            open: false,
            until,
        });
        self.social.dirty.extend([player, other]);
        out.invites.push(InviteOut {
            to: other,
            from: me,
            kind: InviteKind::Trade,
        });
        Ok(())
    }

    pub(crate) fn trade_offer(
        &mut self,
        player: PlayerId,
        items: &[(u16, u32)],
        coins: u64,
    ) -> Result<(), Refusal> {
        let i = self.social.trade_index(player).ok_or(Refusal::NoTrade)?;
        if !self.social.trades.get(i).is_some_and(|t| t.open) {
            return Err(Refusal::NoTrade);
        }
        let handle = *self.by_player.get(&player).ok_or(Refusal::NoTrade)?;
        let cat = &self.content.catalog;
        let mut offers = Vec::with_capacity(items.len());
        {
            let p = self
                .world
                .get::<&PlayerComp>(handle)
                .map_err(|_| Refusal::NoTrade)?;
            let gear = self.world.get::<&Gear>(handle).map_err(|_| Refusal::NoTrade)?;
            if coins > p.data.coins {
                return Err(Refusal::NotEnoughCoins);
            }
            for &(slot, qty) in items {
                if qty == 0 || offers.iter().any(|o: &Offer| o.slot == slot) {
                    return Err(Refusal::InvalidSlot);
                }
                let (item, instance) = match p.data.slots.get(usize::from(slot)) {
                    Some(Slot::Stack { item, qty: have }) if *have >= qty => (*item, None),
                    Some(Slot::Held(item)) if qty == 1 => {
                        let id = gear
                            .bag
                            .iter()
                            .find(|(s, _)| *s == slot)
                            .map(|(_, inst)| inst.id.get())
                            .ok_or(Refusal::InvalidSlot)?;
                        (*item, Some(id))
                    }
                    _ => return Err(Refusal::InvalidSlot),
                };
                // A quest item is not merchandise.
                if cat.item(item).is_none_or(|d| d.kind == ItemKind::Quest) {
                    return Err(Refusal::InvalidSlot);
                }
                offers.push(Offer {
                    slot,
                    qty,
                    item,
                    instance,
                });
            }
        }
        offers.sort_by_key(|o| o.slot);
        let until = self.tick.0 + TRADE_TICKS;
        if let Some(t) = self.social.trades.get_mut(i) {
            if let Some(s) = t.side_mut(player) {
                s.offers = offers;
                s.coins = coins;
            }
            // A new offer takes both confirmations back.
            t.a.confirmed = false;
            t.b.confirmed = false;
            t.until = until;
            self.social.dirty.extend([t.a.player, t.b.player]);
        }
        Ok(())
    }

    pub(crate) fn trade_confirm(&mut self, player: PlayerId, out: &mut TickOutput) -> Result<(), Refusal> {
        let i = self.social.trade_index(player).ok_or(Refusal::NoTrade)?;
        let until = self.tick.0 + TRADE_TICKS;
        let both = {
            let t = self.social.trades.get_mut(i).ok_or(Refusal::NoTrade)?;
            if !t.open {
                return Err(Refusal::NoTrade);
            }
            if let Some(s) = t.side_mut(player) {
                s.confirmed = true;
            }
            t.until = until;
            self.social.dirty.extend([t.a.player, t.b.player]);
            t.a.confirmed && t.b.confirmed
        };
        if !both {
            return Ok(());
        }
        let Some(t) = self.social.trades.get(i).cloned() else {
            return Err(Refusal::NoTrade);
        };
        match self.swap(&t) {
            Ok(()) => {
                self.social.trades.remove(i);
                for p in [t.a.player, t.b.player] {
                    self.tell(out, p, "system.trade_done", smallvec::smallvec![]);
                }
                Ok(())
            }
            Err(e) => {
                // Nothing moved. Both have to look at the offers again.
                if let Some(t) = self.social.trades.get_mut(i) {
                    t.a.confirmed = false;
                    t.b.confirmed = false;
                }
                Err(e)
            }
        }
    }

    pub(crate) fn trade_cancel(&mut self, player: PlayerId, out: &mut TickOutput) {
        self.end_trade(player, Some("system.trade_cancelled"), out);
    }

    /// Ends the trade of `player`, telling both sides with `key`.
    fn end_trade(&mut self, player: PlayerId, key: Option<&str>, out: &mut TickOutput) {
        let Some(i) = self.social.trade_index(player) else {
            return;
        };
        let t = self.social.trades.remove(i);
        self.social.dirty.extend([t.a.player, t.b.player]);
        if let Some(key) = key {
            for p in [t.a.player, t.b.player] {
                self.tell(out, p, key, smallvec::smallvec![]);
            }
        }
    }

    /// Checks the whole swap on copies, then applies it. `Err` changes
    /// nothing at all.
    fn swap(&mut self, t: &Trade) -> Result<(), Refusal> {
        let content = self.content.clone();
        let cat = &content.catalog;
        let ha = *self.by_player.get(&t.a.player).ok_or(Refusal::NoTrade)?;
        let hb = *self.by_player.get(&t.b.player).ok_or(Refusal::NoTrade)?;
        let mut da = self
            .world
            .get::<&PlayerComp>(ha)
            .map(|p| p.data.clone())
            .map_err(|_| Refusal::NoTrade)?;
        let mut db = self
            .world
            .get::<&PlayerComp>(hb)
            .map(|p| p.data.clone())
            .map_err(|_| Refusal::NoTrade)?;
        // What each side still holds must be what it offered.
        for (side, data, handle) in [(&t.a, &da, ha), (&t.b, &db, hb)] {
            if data.coins < side.coins {
                return Err(Refusal::TradeChanged);
            }
            let gear = self.world.get::<&Gear>(handle).map_err(|_| Refusal::NoTrade)?;
            for o in &side.offers {
                let same = match (data.slots.get(usize::from(o.slot)), o.instance) {
                    (Some(Slot::Stack { item, qty }), None) => *item == o.item && *qty >= o.qty,
                    (Some(Slot::Held(item)), Some(id)) => {
                        *item == o.item
                            && gear
                                .bag
                                .iter()
                                .any(|(s, inst)| *s == o.slot && inst.id.get() == id)
                    }
                    _ => false,
                };
                if !same {
                    return Err(Refusal::TradeChanged);
                }
            }
        }
        // Out, then in: coins and stacks on the copies, a free slot picked
        // for every item with identity.
        let mut moved_held: [Vec<(u16, u16)>; 2] = [Vec::new(), Vec::new()];
        moved_held[0] = Self::transfer(&mut da, &mut db, &t.a, cat)?;
        moved_held[1] = Self::transfer(&mut db, &mut da, &t.b, cat)?;
        // Past this point nothing can fail: every instance was seen above.
        let mut inbound: [Vec<(u16, crate::gear::ItemInstance)>; 2] = [Vec::new(), Vec::new()];
        for (n, handle) in [ha, hb].into_iter().enumerate() {
            let Ok(mut gear) = self.world.get::<&mut Gear>(handle) else {
                continue;
            };
            for &(from, to) in moved_held.get(n).map(Vec::as_slice).unwrap_or_default() {
                if let Some(inst) = gear.take_bag(from)
                    && let Some(v) = inbound.get_mut(1 - n)
                {
                    v.push((to, inst));
                }
            }
        }
        let [in_a, in_b] = inbound;
        for (handle, data, inst) in [(ha, da, in_a), (hb, db, in_b)] {
            if let Ok(mut gear) = self.world.get::<&mut Gear>(handle) {
                gear.bag.extend(inst);
            }
            if let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(handle) {
                p.data = data;
                p.dirty_bag = true;
            }
        }
        Ok(())
    }

    /// Moves what `give` offers from `from` to `to`, on copies. Returns,
    /// for every item with identity, its slot in `from` and the free slot
    /// picked in `to`.
    fn transfer(
        from: &mut PlayerData,
        to: &mut PlayerData,
        give: &Side,
        cat: &defs::ValidatedCatalog,
    ) -> Result<Vec<(u16, u16)>, Refusal> {
        let mut held = Vec::new();
        from.coins = from.coins.checked_sub(give.coins).ok_or(Refusal::TradeChanged)?;
        to.coins = to.coins.saturating_add(give.coins);
        for o in &give.offers {
            if o.instance.is_none() {
                from.take(o.item, o.qty).map_err(|_| Refusal::TradeChanged)?;
                to.give(cat, o.item, o.qty).map_err(|_| Refusal::InventoryFull)?;
                continue;
            }
            if let Some(s) = from.slots.get_mut(usize::from(o.slot)) {
                *s = Slot::Empty;
            }
            let free = to
                .slots
                .iter()
                .position(|s| *s == Slot::Empty)
                .ok_or(Refusal::InventoryFull)?;
            if let Some(s) = to.slots.get_mut(free) {
                *s = Slot::Held(o.item);
            }
            held.push((o.slot, u16::try_from(free).unwrap_or(u16::MAX)));
        }
        Ok(held)
    }

    /// A player leaves the zone: its group, invites and trade go with it.
    pub(crate) fn drop_social(&mut self, player: PlayerId) {
        self.social.leave(player);
        self.social.invites.retain(|i| i.from != player && i.to != player);
        if let Some(i) = self.social.trade_index(player) {
            let t = self.social.trades.remove(i);
            self.social.dirty.insert(t.other(player));
        }
        self.social.dirty.remove(&player);
    }

    /// Expires invites and stale trades, cancels trades that walked apart
    /// or into a fight, and sends the views that changed.
    pub(crate) fn run_social(&mut self, out: &mut TickOutput) {
        let now = self.tick.0;
        self.social.invites.retain(|i| i.until > now);
        let mut ending: Vec<(PlayerId, &'static str)> = Vec::new();
        for t in &self.social.trades {
            let (Some(&ha), Some(&hb)) = (self.by_player.get(&t.a.player), self.by_player.get(&t.b.player))
            else {
                ending.push((t.a.player, "system.trade_cancelled"));
                continue;
            };
            if now >= t.until {
                ending.push((t.a.player, "system.trade_expired"));
                continue;
            }
            if !t.open {
                continue;
            }
            let far = match (self.pos_of_handle(ha), self.pos_of_handle(hb)) {
                (Some(x), Some(y)) => x.distance(y) > TRADE_RADIUS,
                _ => true,
            };
            let fighting = [ha, hb].iter().any(|h| {
                self.world
                    .get::<&Fighter>(*h)
                    .is_ok_and(|f| f.target.is_some() || f.fallen_until != 0)
            });
            if far || fighting {
                ending.push((t.a.player, "system.trade_cancelled"));
            }
        }
        for (player, key) in ending {
            self.end_trade(player, Some(key), out);
        }
        for player in std::mem::take(&mut self.social.dirty) {
            if !self.by_player.contains_key(&player) {
                continue;
            }
            let party = self.social.party_of(player).and_then(|g| {
                Some(PartyView {
                    leader: self.entity_of_player(g.leader)?,
                    members: g
                        .members
                        .iter()
                        .filter_map(|m| self.entity_of_player(*m))
                        .collect(),
                })
            });
            out.parties.push((player, party));
            let trade = self.trade_view(player);
            out.trades.push((player, trade));
        }
    }

    fn trade_view(&self, player: PlayerId) -> Option<TradeView> {
        let t = self.social.trades.iter().find(|t| t.has(player))?;
        let other = t.other(player);
        let (me, them) = if t.a.player == player {
            (&t.a, &t.b)
        } else {
            (&t.b, &t.a)
        };
        Some(TradeView {
            with: self.entity_of_player(other)?,
            open: t.open,
            mine: me.offers.iter().map(|o| (o.slot, o.qty)).collect(),
            mine_coins: me.coins,
            theirs: them.offers.iter().map(|o| (o.item, o.qty)).collect(),
            theirs_pieces: {
                let gear = self
                    .by_player
                    .get(&other)
                    .and_then(|h| self.world.get::<&crate::gear::Gear>(*h).ok());
                them.offers
                    .iter()
                    .map(|o| {
                        let g = gear.as_ref()?;
                        o.instance?;
                        let (_, i) = g.bag.iter().find(|(s, _)| *s == o.slot)?;
                        Some(i.view(o.slot, &self.content.catalog))
                    })
                    .collect()
            },
            theirs_coins: them.coins,
            mine_confirmed: me.confirmed,
            theirs_confirmed: them.confirmed,
        })
    }
}
