//! From a loaded character to a player in the zone: results of the
//! account tasks, the entry queue, reloaded content, leaving, and the
//! batch save of everyone in the zone.

use super::account::failed;
use super::*;

impl Gateway {
    /// New content passed every check: the zone switches first; clients
    /// hear of it when the zone confirms (`FromZone::Reloaded`).
    pub(super) fn on_reload(&mut self, r: Reloaded) {
        let hash = r.frames.hash;
        *self
            .cfg
            .slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = r.content;
        self.pending_content = Some(r.frames);
        self.pending_reload = self.zones.keys().copied().collect();
        for z in self.zones.values() {
            let _ = z.tx.send(ToZone::Reload { content_hash: hash });
        }
        self.check_reload_done();
    }

    /// Every zone confirmed the pending content (or there is none to wait
    /// for): clients switch.
    pub(super) fn check_reload_done(&mut self) {
        if self.pending_reload.is_empty()
            && let Some(hash) = self.pending_content.as_ref().map(|f| f.hash)
        {
            self.on_content_switched(hash);
        }
    }

    /// The zone runs the new content: text packs, definitions and keys
    /// switch, and every player in the game is told.
    pub(super) fn on_content_switched(&mut self, content_hash: u64) {
        let Some(frames) = self.pending_content.take_if(|f| f.hash == content_hash) else {
            return;
        };
        self.keys = Arc::new(frames.keys.clone());
        self.cfg.content = frames;
        let mut changed = Vec::new();
        if protocol::encode(&ServerMsg::TextPackChanged { content_hash }, &mut changed).is_err() {
            return;
        }
        let changed = Bytes::from(changed);
        for (&c, conn) in &mut self.conns {
            conn.packs_sent.clear();
            if matches!(conn.phase, Phase::InZone(_)) {
                self.net.send(c, changed.clone());
                self.net.send(c, self.cfg.content.definitions.clone());
            }
        }
        self.refresh_chunks();
    }

    /// One more wrong password server wide: past the limit, the shield
    /// asks every new `Login` and `Register` for a proof of work.
    fn wrong_password(&mut self) {
        if self.shield.failed(login::now_unix()) {
            warn!(
                bits = login::SHIELD_BITS,
                "many wrong passwords: proof of work on"
            );
        }
    }

    pub(super) fn on_done(&mut self, done: Done) {
        match done {
            Done::Session {
                c,
                account,
                role,
                account_name,
                token,
                characters,
                after,
                registered,
                must_change,
            } => {
                for key in &after.clear {
                    self.attempts.succeeded(key);
                }
                for key in &after.refund {
                    self.attempts.refund(key);
                }
                let Some(conn) = self.conns.get_mut(&c) else {
                    return;
                };
                if registered {
                    self.registrations.created(conn.peer.ip(), Instant::now());
                }
                conn.account = Some(account);
                conn.must_change = must_change;
                conn.role = role;
                conn.account_name = account_name;
                conn.phase = Phase::Authenticated;
                let characters = self.summaries(characters);
                self.net.send_msg(
                    c,
                    &ServerMsg::Session {
                        token,
                        characters,
                        device: after.device,
                        failed_logins: after.failed_logins,
                    },
                );
                self.net.send(c, self.cfg.content.appearances.clone());
            }
            // The try was counted when it arrived; here only the
            // shield hears of it, and the line is what fail2ban reads.
            Done::ChangeFailed { c, account, ip } => {
                self.wrong_password();
                let Some(conn) = self.conns.get_mut(&c) else {
                    return;
                };
                conn.phase = Phase::Authenticated;
                warn!(ip = %conn.peer.ip(), %account, key = %ip, "wrong current password");
                self.reply_error(c, ErrorCode::WrongCredentials, Vec::new());
            }
            Done::LoginFailed { c, account, ip } => {
                self.wrong_password();
                let Some(conn) = self.conns.get_mut(&c) else {
                    return;
                };
                conn.phase = Phase::AwaitLogin;
                warn!(ip = %conn.peer.ip(), %account, key = %ip, "login failed");
                self.reply_error(c, ErrorCode::WrongCredentials, Vec::new());
            }
            Done::Failed { c, code, args, back } => {
                if let Back::AuthenticatedRelease(account) = back {
                    self.online_accounts.remove(&account);
                }
                let Some(conn) = self.conns.get_mut(&c) else {
                    return;
                };
                conn.phase = match back {
                    Back::Close => return self.fail(c, code),
                    Back::Login => Phase::AwaitLogin,
                    Back::Authenticated | Back::AuthenticatedRelease(_) => Phase::Authenticated,
                };
                self.reply_error(c, code, args);
            }
            Done::Characters { c, characters } => {
                let Some(conn) = self.conns.get_mut(&c) else {
                    return;
                };
                conn.phase = Phase::Authenticated;
                let characters = self.summaries(characters);
                self.net.send_msg(c, &ServerMsg::Characters { characters });
            }
            Done::Loaded {
                c,
                character,
                dev,
                muted_until,
            } => {
                if let Some(conn) = self.conns.get_mut(&c) {
                    conn.muted_until = muted_until;
                }
                // The development login has no `Session`, which carries the
                // look lists: without them every player is drawn plain.
                if dev {
                    self.net.send(c, self.cfg.content.appearances.clone());
                }
                self.on_loaded(c, character, dev);
            }
            Done::Released { account, character } => {
                self.release(account, character);
            }
            Done::Polled { sanctions, admin } => self.on_polled(sanctions, admin),
            Done::Muted { account, until } => {
                for conn in self.conns.values_mut() {
                    if conn.account == Some(account) {
                        conn.muted_until = until;
                    }
                }
            }
            Done::Reply { c, key, args, poll } => {
                self.system(c, key, args);
                if poll {
                    self.poll_admin_changes();
                }
            }
            Done::Error { c, code } => self.reply_error(c, code, Vec::new()),
            Done::EscrowTaken {
                player,
                character,
                escrow,
                parcel,
            } => self.on_escrow_taken(player, character, escrow, parcel),
            Done::FirstFeat { name, title } => self.on_first_feat(&name, &title),
            Done::Claimed {
                player,
                character,
                key,
                holder,
            } => self.on_claimed(player, character, key, holder),
            Done::Holders(holders) => self.on_holders(holders),
            Done::DungeonFirst { party, name } => self.on_dungeon_first(party, name),
            Done::Ranked { lists, at } => self.on_ranked(lists, at),
            Done::Firsts { c, rows } => self.on_firsts_read(c, &rows),
        }
    }

    pub(super) fn on_loaded(&mut self, c: ConnectionId, ch: Box<LoadedCharacter>, dev: bool) {
        let ch = *ch;
        let account = (!dev).then_some(ch.account);
        let Some(conn) = self.conns.get_mut(&c) else {
            // Gone while loading: undo the reservation.
            if let Some(a) = account {
                self.online_accounts.remove(&a);
            }
            return;
        };
        if self.online_characters.contains(&ch.id) {
            if let Some(a) = account {
                self.online_accounts.remove(&a);
            }
            if dev {
                return self.fail(c, ErrorCode::NameTaken);
            }
            conn.phase = Phase::Authenticated;
            return self.reply_error(c, ErrorCode::AlreadyOnline, Vec::new());
        }
        // A dead character rests; other characters may play.
        let now = login::now_unix();
        if ch.sheet.locked_until > now {
            if let Some(a) = account {
                self.online_accounts.remove(&a);
            }
            let hours = (ch.sheet.locked_until - now).div_ceil(3600);
            let args = vec![Arg::Int(i64::try_from(hours).unwrap_or(i64::MAX))];
            if dev {
                return self.kick(c, ErrorCode::CharacterResting, args);
            }
            conn.phase = Phase::Authenticated;
            return self.reply_error(c, ErrorCode::CharacterResting, args);
        }
        self.online_characters.insert(ch.id);
        // Phase 10: the zone that owns the saved tile, whatever zone name
        // it was saved under (it may have been past the seam); none (new
        // character, or a zone removed): the start zone's spawn.
        let content = crate::content::slot_get(&self.cfg.slot);
        let (zone, pos) = ch
            .place
            .as_ref()
            .and_then(|(_, pos)| {
                let (id, _) = content.catalog.zones().find(|(_, z)| z.contains(*pos))?;
                Some((id, Some(*pos)))
            })
            .unwrap_or((self.cfg.start_zone, None));
        conn.account = conn.account.or(Some(ch.account));
        conn.pending = Some(Pending {
            character: ch.id,
            sheet: translation::sheet_to_sim(ch.name, &ch.sheet),
            zone,
            pos,
            account,
        });
        conn.phase = Phase::Queued;
        self.queue.push(c);
        self.pump_queue();
        self.send_queue_positions();
    }

    pub(super) fn has_room(&self) -> bool {
        self.in_game.len() < self.cfg.max_players
            && self.load() != LoadLevel::N3
            && self.zones.contains_key(&self.cfg.start_zone)
    }

    /// Lets the head of the queue in while there is room.
    pub(super) fn pump_queue(&mut self) {
        while self.has_room() {
            let Some(c) = self.queue.pop(Instant::now()) else {
                return;
            };
            self.enter(c);
        }
    }

    pub(super) fn enter(&mut self, c: ConnectionId) {
        let Some(conn) = self.conns.get_mut(&c) else {
            return;
        };
        let Some(mut p) = conn.pending.take() else { return };
        // The saved zone is down: the start zone's spawn instead.
        if !self.zones.contains_key(&p.zone) {
            p.zone = self.cfg.start_zone;
            p.pos = None;
        }
        let player = PlayerId::new(self.next_player);
        self.next_player += 1;
        conn.phase = Phase::Entering(player);
        self.players.insert(player, c);
        self.in_game.insert(
            player,
            InGame {
                character: p.character,
                account: p.account,
                name: p.sheet.name.clone(),
                zone: p.zone,
                crossing: None,
            },
        );
        // What others hold goes first: the zone checks titles on entering.
        let keys = unique::keys_of(&p.sheet, &self.cfg.content.unique);
        self.send_taken(player, p.character);
        self.to_player(
            player,
            Command::Enter {
                player,
                sheet: Box::new(p.sheet),
                pos: p.pos,
            },
        );
        self.claim_all(player, p.character, keys);
    }

    pub(super) fn send_queue_positions(&mut self) {
        for (c, position, estimate) in self.queue.positions(Instant::now()) {
            let Some(conn) = self.conns.get_mut(&c) else {
                continue;
            };
            if conn.queue_sent.map(|(p, _)| p) == Some(position) {
                continue;
            }
            conn.queue_sent = Some((position, estimate));
            self.net.send_msg(
                c,
                &ServerMsg::QueuePosition {
                    position,
                    estimate_secs: estimate,
                },
            );
        }
    }

    /// `staying`: the zone's other players at the same tick, saved in the
    /// same transaction. An item handed over just before the
    /// leave then moves in one step: never refused on the receiver's side
    /// while the giver's next save drops it.
    pub(super) fn on_left(
        &mut self,
        player: PlayerId,
        sheet: Box<CharacterSheet>,
        pos: TilePos,
        staying: Vec<(PlayerId, CharacterSheet, TilePos)>,
    ) {
        let sheet = *sheet;
        self.players.remove(&player);
        let Some(who) = self.in_game.remove(&player) else {
            return;
        };
        let mut saves = vec![CharacterSave {
            id: who.character,
            zone: self.place(who.zone, pos).0,
            pos: self.place(who.zone, pos).1,
            sheet: translation::sheet_to_saved(&sheet),
        }];
        saves.extend(self.saves_of(staying));
        self.save(SaveJob {
            saves,
            release: vec![(who.account, who.character)],
            escrow: None,
        });
        self.pump_queue();
    }

    /// Saves for the players the gateway still has in the game.
    fn saves_of(&self, list: Vec<(PlayerId, CharacterSheet, TilePos)>) -> Vec<CharacterSave> {
        list.into_iter()
            .filter_map(|(player, sheet, pos)| {
                self.in_game.get(&player).map(|who| CharacterSave {
                    id: who.character,
                    zone: self.place(who.zone, pos).0,
                    pos: self.place(who.zone, pos).1,
                    sheet: translation::sheet_to_saved(&sheet),
                })
            })
            .collect()
    }

    pub(super) fn save_checkpoint(&mut self, list: Vec<(PlayerId, CharacterSheet, TilePos)>) {
        let saves = self.saves_of(list);
        if !saves.is_empty() {
            self.save(SaveJob {
                saves,
                release: Vec::new(),
                escrow: None,
            });
        }
    }

    pub(super) fn reload_after_refusal(&self, c: ConnectionId, who: InGame) {
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        tokio::spawn(async move {
            let muted_until = match who.account {
                Some(a) => repo.mute_until(a, login::now_unix()).await.ok().flatten(),
                None => None,
            };
            let result = match repo.load_character(who.character).await {
                Ok(Some(ch)) => Done::Loaded {
                    c,
                    dev: who.account.is_none(),
                    character: Box::new(ch),
                    muted_until,
                },
                Ok(None) => Done::Failed {
                    c,
                    code: ErrorCode::NoSuchCharacter,
                    args: Vec::new(),
                    back: Back::Close,
                },
                Err(e) => failed(c, e, Back::Close),
            };
            let _ = done.send(result);
        });
    }

    /// The text pack of the running content in one locale,
    /// sent once per connection and content.
    pub(super) fn send_text_pack(&mut self, c: ConnectionId, locale: Locale) {
        let fresh = self
            .conns
            .get_mut(&c)
            .is_some_and(|conn| conn.packs_sent.insert(locale));
        // A language the content does not have gets English.
        let packs = &self.cfg.content.text_packs;
        if fresh && let Some(parts) = packs.get(&locale).or_else(|| packs.get(&Locale::SOURCE)) {
            for part in parts {
                self.net.send(c, part.clone());
            }
        }
    }
}
