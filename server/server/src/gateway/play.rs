//! Playing: the messages of a player already in the zone, and the frames
//! the zone sends back.

use super::chunks::in_range;
use super::*;

impl Gateway {
    pub(super) fn on_zone_event(&mut self, ev: ZoneEvent) {
        match ev {
            ZoneEvent::Up { zone, tx } => {
                if self.zones.contains_key(&zone) {
                    warn!(zone = %self.zone_name(zone), "zone linked twice: the new link is dropped");
                    return;
                }
                info!(zone = %self.zone_name(zone), "zone up");
                // A zone that comes up during a reload takes the content too.
                if let Some(f) = &self.pending_content {
                    let _ = tx.send(ToZone::Reload { content_hash: f.hash });
                    self.pending_reload.insert(zone);
                }
                self.zones.insert(
                    zone,
                    ZoneConn {
                        tx,
                        load: LoadLevel::N0,
                    },
                );
                self.send_taken_items(Some(zone));
                self.send_goals_done(Some(zone));
                self.on_instance_up(zone);
                self.pump_queue();
            }
            ZoneEvent::Msg(zone, fz) => self.on_zone(zone, fz),
            ZoneEvent::Down(zone) => self.on_zone_down(zone),
        }
    }

    /// A zone's link closed: its players go back to their last save
    fn on_zone_down(&mut self, zone: ZoneId) {
        if self.zones.remove(&zone).is_none() {
            return;
        }
        error!(zone = %self.zone_name(zone), "zone down: its players are dropped");
        self.pending_reload.remove(&zone);
        let players: Vec<PlayerId> = self
            .in_game
            .iter()
            .filter(|(_, w)| w.zone == zone)
            .map(|(p, _)| *p)
            .collect();
        self.on_zone(zone, FromZone::Failure { players });
        self.check_reload_done();
    }

    pub(super) fn on_zone(&mut self, zone: ZoneId, fz: FromZone) {
        match fz {
            FromZone::Entered {
                player,
                entity,
                pos,
                tick,
            } => {
                let Some(&c) = self.players.get(&player) else {
                    return;
                };
                let Some(conn) = self.conns.get_mut(&c) else {
                    return;
                };
                conn.pos = pos;
                // Phase 10: a player handed over by the seam is already in
                // the game; the client must not notice the new zone.
                if matches!(conn.phase, Phase::InZone(_)) {
                    self.stream_chunks(c);
                    if let Some(at) = self.in_game.get_mut(&player).and_then(|w| w.crossing.take()) {
                        let ms = at.elapsed().as_millis();
                        if ms >= CROSSING_SLOW_MS {
                            warn!(player = player.get(), ms, "slow seam crossing");
                        } else {
                            tracing::debug!(player = player.get(), ms, "seam crossing");
                        }
                    }
                    return;
                }
                conn.phase = Phase::InZone(player);
                // Rule 4: what custody keeps for it arrives now.
                if let Some(who) = self.in_game.get(&player) {
                    self.deliver_waiting(player, who.character);
                }
                self.net.send_msg(
                    c,
                    &ServerMsg::Welcome {
                        your_id: entity,
                        pos,
                        tick,
                        content_hash: self.cfg.content.hash,
                        view_radius: self.cfg.view_radius,
                    },
                );
                self.net.send(c, self.cfg.content.definitions.clone());
                self.net.send_msg(c, &self.world_time());
                self.stream_chunks(c);
            }
            FromZone::Frames(frames) => {
                for (player, frame) in frames {
                    if let Some(&c) = self.players.get(&player) {
                        self.net.send(c, frame);
                    }
                }
            }
            FromZone::Tiles(tiles) => self.switch_tiles(&tiles),
            FromZone::Moved(moved) => {
                for (player, pos) in moved {
                    let Some(&c) = self.players.get(&player) else {
                        continue;
                    };
                    if let Some(conn) = self.conns.get_mut(&c) {
                        conn.pos = pos;
                    }
                    self.stream_chunks(c);
                }
            }
            FromZone::Left {
                player,
                sheet,
                pos,
                staying,
            } => self.on_left(player, sheet, pos, staying),
            FromZone::Checkpoint(list) => {
                self.save_checkpoint(list);
                self.poll_admin_changes();
            }
            // Economy telemetry (phase 7): one row per source per
            // checkpoint, written beside the saves.
            FromZone::Coins(rows) => {
                let repo = self.repo.clone();
                let at = crate::login::now_unix();
                let rows: Vec<persistence::CoinFlowRow> = rows
                    .into_iter()
                    .map(|(flow, amount)| persistence::CoinFlowRow {
                        at,
                        flow: flow.name().to_owned(),
                        created: flow.creates(),
                        amount,
                    })
                    .collect();
                tokio::spawn(async move {
                    if let Err(e) = repo.save_coin_flow(rows).await {
                        tracing::warn!(error = %e, "coin telemetry not written");
                    }
                });
            }
            FromZone::Load(level) => {
                if let Some(z) = self.zones.get_mut(&zone) {
                    z.load = level;
                }
                self.pump_queue();
            }
            FromZone::Refused { player, .. } => {
                // The zone hit N3 before the gateway knew: back in the queue.
                let who = self.in_game.remove(&player);
                let Some(c) = self.players.remove(&player) else {
                    return;
                };
                let Some(conn) = self.conns.get_mut(&c) else {
                    return;
                };
                if let (Some(who), Phase::Entering(_)) = (who, &conn.phase) {
                    warn!(?player, "zone refused the entry: back in the queue");
                    // The sheet went to the zone; load it again from disk.
                    conn.phase = Phase::Busy;
                    // The account stays reserved; the character is taken again on load.
                    self.online_characters.remove(&who.character);
                    self.reload_after_refusal(c, who);
                }
            }
            FromZone::Reloaded { content_hash } => {
                if self
                    .pending_content
                    .as_ref()
                    .is_some_and(|f| f.hash == content_hash)
                {
                    self.pending_reload.remove(&zone);
                    self.check_reload_done();
                }
            }
            FromZone::NeedItemIds => {
                let Some(tx) = self.zones.get(&zone).map(|z| z.tx.clone()) else {
                    return;
                };
                let repo = self.repo.clone();
                tokio::spawn(async move {
                    match repo.reserve_item_ids(ITEM_ID_BLOCK).await {
                        Ok((start, end)) => {
                            let _ = tx.send(ToZone::Command(Command::ItemIds { start, end }));
                        }
                        Err(e) => warn!(error = %e, "item ids not reserved"),
                    }
                });
            }
            FromZone::Dead { player, hours } => {
                // Saved with the lock on the way out (Disconnected -> Leave).
                if let Some(&c) = self.players.get(&player) {
                    self.kick(c, ErrorCode::CharacterResting, vec![Arg::Int(i64::from(hours))]);
                }
            }
            FromZone::Hello { .. } => {}
            FromZone::HandOff { player, to, arrival } => self.on_handoff(zone, player, to, arrival),
            FromZone::Ghosts { to, diff } => {
                self.to_zone(to, ToZone::Command(Command::Ghosts { from: zone, diff }));
            }
            FromZone::Escrow(e) => self.on_escrow(zone, *e),
            FromZone::EscrowLost { from, parcel, .. } => self.on_escrow_lost(from, &parcel),
            FromZone::Delivered(d) => self.on_delivered(zone, *d),
            FromZone::Feats(feats) => self.on_feats(feats),
            FromZone::Claims(claims) => self.on_claims(claims),
            FromZone::Clears(clears) => self.on_clears(clears),
            FromZone::Released(keys) => self.on_released(keys),
            FromZone::Goals(counts) => self.on_goal_counts(counts),
            FromZone::Failure { players } => {
                for player in players {
                    // The zone already forgot this player: no Leave, no save.
                    if let Some(who) = self.in_game.remove(&player) {
                        self.release(who.account, who.character);
                    }
                    if let Some(c) = self.players.remove(&player) {
                        if let Some(conn) = self.conns.get_mut(&c) {
                            conn.phase = Phase::AwaitLogin;
                        }
                        self.fail(c, ErrorCode::ZoneFailure);
                    }
                }
                self.pump_queue();
            }
        }
    }

    /// A player crossed the seam (phase 10): the next zone gets it. With
    /// that zone down the player is saved where it stands and dropped.
    fn on_handoff(&mut self, from: ZoneId, player: PlayerId, to: ZoneId, arrival: Box<sim::Arrival>) {
        // A dungeon the character is locked out of: back where it was.
        let (to, arrival) = self.turn_back_if_locked(player, from, to, arrival);
        // An instance: the party's own copy, maybe still on its way.
        let Some((to, arrival)) = self.route_arrival(player, to, arrival) else {
            return;
        };
        if !self.zones.contains_key(&to) {
            warn!(from = %self.zone_name(from), to = %self.zone_name(to), "handoff to a zone that is down: player saved and dropped");
            if let Some(&c) = self.players.get(&player) {
                self.net.send_error_and_close(c, ErrorCode::ZoneFailure);
                if let Some(conn) = self.conns.get_mut(&c) {
                    conn.phase = Phase::AwaitLogin;
                }
            }
            let pos = arrival.left_from.unwrap_or(arrival.pos);
            let sheet = arrival.sheet;
            // Saved under the zone it left: that zone's map has the tile.
            return self.on_left(player, Box::new(sheet), pos, Vec::new());
        }
        if let Some(w) = self.in_game.get_mut(&player) {
            w.zone = to;
            w.crossing = Some(Instant::now());
        }
        if let Some(character) = self.in_game.get(&player).map(|w| w.character) {
            self.send_taken(player, character);
        }
        self.to_zone(to, ToZone::Command(Command::Arrive { player, arrival }));
    }

    /// A player already in the zone: everything that becomes a zone
    /// command, and the terrain they ask for.
    pub(super) fn on_play_msg(&mut self, c: ConnectionId, msg: ClientMsg) {
        let Some(conn) = self.conns.get(&c) else {
            return;
        };
        let pos = conn.pos;
        match (&conn.phase, msg) {
            (Phase::InZone(player), ClientMsg::Move { destination }) => {
                let player = *player;
                self.to_player(player, Command::Move { player, destination });
            }
            (Phase::InZone(player), ClientMsg::Stop) => {
                let player = *player;
                self.to_player(player, Command::Stop { player });
            }
            (Phase::InZone(player), ClientMsg::Talk { target }) => {
                let player = *player;
                self.to_player(player, Command::Talk { player, target });
            }
            (Phase::InZone(player), ClientMsg::ChooseOption { option }) => {
                let player = *player;
                self.to_player(player, Command::ChooseOption { player, option });
            }
            (Phase::InZone(player), ClientMsg::MoveItem { from, to }) => {
                let player = *player;
                self.to_player(player, Command::MoveItem { player, from, to });
            }
            (
                Phase::InZone(player),
                msg @ (ClientMsg::Attack { .. }
                | ClientMsg::UseTechnique { .. }
                | ClientMsg::AllocatePoints { .. }
                | ClientMsg::ChooseClass { .. }
                | ClientMsg::Equip { .. }
                | ClientMsg::Unequip { .. }
                | ClientMsg::Appraise { .. }
                | ClientMsg::UseItem { .. }
                | ClientMsg::UseItemOn { .. }
                | ClientMsg::CloseDialogue
                | ClientMsg::OpenShop { .. }
                | ClientMsg::CloseShop
                | ClientMsg::Buy { .. }
                | ClientMsg::Sell { .. }
                | ClientMsg::Repair { .. }
                | ClientMsg::ShopAppraise { .. }
                | ClientMsg::SelfRepair { .. }
                | ClientMsg::Craft { .. }
                | ClientMsg::Gather { .. }
                | ClientMsg::UseTile { .. }
                | ClientMsg::PartyInvite { .. }
                | ClientMsg::AnswerInvite { .. }
                | ClientMsg::LeaveParty
                | ClientMsg::PartyKick { .. }
                | ClientMsg::PartyLead { .. }
                | ClientMsg::TradeRequest { .. }
                | ClientMsg::TradeOffer { .. }
                | ClientMsg::TradeConfirm
                | ClientMsg::TradeCancel
                | ClientMsg::Emote { .. }
                | ClientMsg::Tame { .. }
                | ClientMsg::Summon { .. }
                | ClientMsg::Dismiss
                | ClientMsg::ReleaseCompanion { .. }
                | ClientMsg::NameCompanion { .. }
                | ClientMsg::Rest { .. }
                | ClientMsg::GiveUp),
            ) => {
                let player = *player;
                if let Some(cmd) = translation::fight_command(player, msg) {
                    self.to_player(player, cmd);
                }
            }
            (Phase::InZone(_), ClientMsg::RequestChunk { cx, cy }) => {
                if in_range(self.cfg.view_radius, pos, cx, cy)
                    && let Some(frame) = self.chunks.get(&(cx, cy))
                {
                    self.net.send(c, frame.clone());
                }
            }
            _ => self.fail(c, ErrorCode::InvalidMessage),
        }
    }
}
