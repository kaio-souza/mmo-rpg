//! The `commands` system: every [`Command`] of the tick, applied in the
//! order it arrived.
//!
//! Entering and leaving are commands too, so replaying the command list
//! rebuilds the zone ([ ]).

use super::*;

impl Zone {
    pub(super) fn apply(&mut self, cmd: &Command, out: &mut TickOutput) {
        match cmd {
            Command::Enter { player, sheet, pos } => {
                if self.by_player.contains_key(player) {
                    return;
                }
                let id = EntityId::new(self.next_id);
                self.next_id = self.next_id.saturating_add(1);
                // A saved tile taken by someone else: the nearest free one.
                let pos = pos
                    .map(|p| self.nearest_free(p))
                    .filter(|p| self.free(*p))
                    .unwrap_or_else(|| self.spawn_point(id));
                self.enter_player(*player, id, pos, sheet, out);
            }
            Command::Arrive { player, arrival } => {
                if self.by_player.contains_key(player) || self.by_id.contains_key(&arrival.entity) {
                    return;
                }
                let id = arrival.entity;
                let pos = Some(self.nearest_free(arrival.pos))
                    .filter(|p| self.free(*p))
                    .unwrap_or_else(|| self.spawn_point(id));
                let Some(h) = self.enter_player(*player, id, pos, &arrival.sheet, out) else {
                    return;
                };
                let (from, grid) = (pos, &self.grid);
                let steps = arrival.destination.and_then(|to| {
                    world::path_avoiding(&self.map, from, to, PATH_MAX_NODES, |t| {
                        crate::movement::occupied_near(grid, from, to, t)
                    })
                });
                // The tick in transit counts toward the wait: the walk keeps
                // its pace across the seam (no step on arrival).
                let ready = self.tick.0.saturating_add(arrival.step_wait.saturating_sub(1));
                if let Ok((f, body, walk, p)) =
                    self.world
                        .query_one_mut::<(&mut Fighter, &mut Body, &mut Walk, &mut PlayerComp)>(h)
                {
                    f.vigor = arrival.vigor.min(f.max_vigor);
                    f.mp = arrival.mp.min(f.max_mp);
                    body.facing = arrival.facing;
                    walk.steps = steps.map(VecDeque::from).unwrap_or_default();
                    walk.ready = ready;
                    // The client already holds this backpack. Quests and
                    // shops stay dirty: their markers name this zone's NPCs.
                    p.dirty_bag = false;
                }
            }
            Command::Ghosts { from, diff } => self.apply_ghosts(*from, diff),
            Command::EscrowDeposit {
                player,
                from,
                items,
                coins,
                to,
            } => {
                if let Err(reason) = self.escrow_deposit(*player, *from, items, *coins, to, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::EscrowDone { ticket, ok } => self.escrow_done(*ticket, *ok, out),
            Command::Deliver {
                player,
                escrow,
                parcel,
            } => self.deliver(*player, *escrow, parcel, out),
            Command::Leave { player } => {
                if let Some((sheet, pos, _)) = self.take_player(*player) {
                    out.left.push((*player, sheet, pos));
                }
            }
            Command::Move { player, destination } => {
                let Some(&handle) = self.by_player.get(player) else {
                    return;
                };
                if !self.standing(handle) {
                    return;
                }
                self.cancel_attack(handle);
                let Ok((body, walk)) = self.world.query_one_mut::<(&Body, &mut Walk)>(handle) else {
                    return;
                };
                if body.pos.distance(*destination) > MAX_MOVE_DISTANCE {
                    return;
                }

                let (from, to, grid) = (body.pos, *destination, &self.grid);
                walk.steps = world::path_avoiding(&self.map, from, to, PATH_MAX_NODES, |t| {
                    crate::movement::occupied_near(grid, from, to, t)
                })
                .map(VecDeque::from)
                .unwrap_or_default();
                walk.blocked = 0;
            }
            Command::Stop { player } => {
                let Some(&handle) = self.by_player.get(player) else {
                    return;
                };
                if let Ok(walk) = self.world.query_one_mut::<&mut Walk>(handle) {
                    walk.steps.clear();
                }
                self.cancel_attack(handle);
            }
            Command::Talk { player, target } => {
                let result = self.talk(*player, *target);
                out.dialogue
                    .push(result.unwrap_or_else(|reason| DialogueOut::Refused {
                        player: *player,
                        reason,
                    }));
            }
            Command::ChooseOption { player, option } => {
                let result = self.choose(*player, *option);
                out.dialogue
                    .push(result.unwrap_or_else(|reason| DialogueOut::Refused {
                        player: *player,
                        reason,
                    }));
            }
            Command::OpenShop { player, target } => {
                if let Err(reason) = self.open_shop(*player, *target) {
                    out.refused.push((*player, reason));
                }
            }
            Command::CloseShop { player } => self.close_shop(*player, out),
            Command::Buy { player, index, qty } => {
                if let Err(reason) = self.buy(*player, *index, *qty, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::Sell { player, slot, qty } => {
                if let Err(reason) = self.sell(*player, *slot, *qty, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::Repair { player, slot } => {
                if let Err(reason) = self.repair(*player, *slot, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::ShopAppraise { player, slot } => {
                if let Err(reason) = self.shop_appraise(*player, *slot, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::SelfRepair { player, slot } => {
                if let Err(reason) = self.self_repair(*player, *slot, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::Craft { player, recipe } => {
                if let Err(reason) = self.craft(*player, *recipe, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::Gather { player, target } => {
                if let Err(reason) = self.gather(*player, *target, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::UseTile { player, pos } => {
                if let Err(reason) = self.use_tile(*player, *pos, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::PartyInvite { player, target } => {
                if let Err(reason) = self.party_invite(*player, *target, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::AnswerInvite { player, accept } => {
                if let Err(reason) = self.answer_invite(*player, *accept, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::LeaveParty { player } => self.leave_party(*player, out),
            Command::PartyKick { player, target } => {
                if let Err(reason) = self.party_kick(*player, *target, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::PartyLead { player, target } => {
                if let Err(reason) = self.party_lead(*player, *target) {
                    out.refused.push((*player, reason));
                }
            }
            Command::TradeRequest { player, target } => {
                if let Err(reason) = self.trade_request(*player, *target, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::TradeOffer { player, items, coins } => {
                if let Err(reason) = self.trade_offer(*player, items, *coins) {
                    out.refused.push((*player, reason));
                }
            }
            Command::TradeConfirm { player } => {
                if let Err(reason) = self.trade_confirm(*player, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::TradeCancel { player } => self.trade_cancel(*player, out),
            Command::Emote { player, emote } => {
                if let Err(reason) = self.emote(*player, *emote, out) {
                    out.refused.push((*player, reason));
                }
            }
            Command::CloseDialogue { player } => {
                let Some(&handle) = self.by_player.get(player) else {
                    return;
                };
                if let Ok(open) = self.world.remove_one::<DialogueComp>(handle) {
                    out.dialogue.push(DialogueOut::End {
                        player: *player,
                        npc: open.npc,
                    });
                }
            }
            Command::MoveItem { player, from, to } => {
                let Some(&handle) = self.by_player.get(player) else {
                    return;
                };
                if let Some(r) = self.move_held(handle, *from, *to) {
                    if let Err(e) = r {
                        out.refused.push((*player, e));
                    }
                    return;
                }
                let content = self.content.clone();
                let Ok(p) = self.world.query_one_mut::<&mut PlayerComp>(handle) else {
                    return;
                };
                match p
                    .data
                    .move_item(&content.catalog, usize::from(*from), usize::from(*to))
                {
                    Ok(()) => p.dirty_bag = true,
                    Err(_) => out.refused.push((*player, Refusal::InvalidSlot)),
                }
            }
            other => {
                if let Err((player, e)) = self.apply_fight(other, out) {
                    out.refused.push((player, e));
                }
            }
        }
    }

    /// Puts a player in the zone with this id on this tile (`Enter` and
    /// `Arrive`).
    fn enter_player(
        &mut self,
        player: PlayerId,
        id: EntityId,
        pos: TilePos,
        sheet: &CharacterSheet,
        out: &mut TickOutput,
    ) -> Option<hecs::Entity> {
        let cat = &self.content.catalog;
        let mut data = PlayerData::import(sheet, cat);
        let mut gear = Gear::import(&sheet.items, cat);
        place_bag(&mut data, &mut gear, cat);
        let mut growth = Growth::import(&sheet.progress, cat);
        // A title added to the content since the last visit is granted on
        // entering, from the saved counters. Before the fighter,
        // so its bonuses count in the maximums.
        let titles = growth.check_titles(cat);
        let fighter = fight::player_fighter(self, &growth, &gear, sheet.hp);
        let h = self.insert_entity(
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
                PlayerComp {
                    player,
                    sheet: sheet.clone(),
                    data,
                    known: Vec::new(),
                    dirty_bag: true,
                    dirty_quests: true,
                    growth,
                    dirty_stats: true,
                    locked_until: sheet.locked_until,
                    item_ready: Vec::new(),
                    dirty_shops: true,
                    emote_ready: 0,
                },
                fighter,
                gear,
            ),
        );
        out.entered.push((player, id, pos));
        fight::announce(self, player, &titles, out);
        Some(h)
    }

    /// Takes a player out of the zone with its sheet written back: on
    /// `Leave` and on a handoff. The groups and trades of the zone drop it.
    pub(crate) fn take_player(&mut self, player: PlayerId) -> Option<(CharacterSheet, TilePos, EntityId)> {
        let handle = *self.by_player.get(&player)?;
        let hp = self.world.get::<&Fighter>(handle).map_or(0, |f| f.hp);
        // The backpack leaves by value: an item has no `Clone`.
        let gear = self.world.remove_one::<Gear>(handle).unwrap_or_default();
        let leaving = (|| {
            let p = self.world.get::<&PlayerComp>(handle).ok()?;
            let b = self.world.get::<&Body>(handle).ok()?;
            let mut sheet = self.export(&p, &gear);
            sheet.hp = hp;
            Some((sheet, b.pos, b.id))
        })();
        self.drop_social(player);
        match leaving {
            Some((sheet, pos, id)) => {
                self.remove_entity(id);
                Some((sheet, pos, id))
            }
            // No body or no sheet to save: it still leaves both indexes,
            // so neither keeps a dead handle.
            None => {
                match self.id_of(handle) {
                    Some(id) => {
                        self.remove_entity(id);
                    }
                    None => {
                        self.by_player.remove(&player);
                        let _ = self.world.despawn(handle);
                    }
                }
                None
            }
        }
    }

    /// A move or stop: the weapon stops following the target (it would
    /// walk the player back to it), and retaliation waits 2 s
    /// (`combate-automatico.md`).
    fn cancel_attack(&mut self, h: hecs::Entity) {
        let now = self.tick.0;
        if let Ok(f) = self.world.query_one_mut::<&mut Fighter>(h) {
            f.target = None;
            f.retaliating = false;
            f.last_order = now;
        }
    }
}
