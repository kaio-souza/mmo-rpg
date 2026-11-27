//! The game itself: world, HUD, chat, keys and clicks. Before the game the
//! window shows the screens of `flow`.

use crate::{App, MAX_ZOOM, ZOOM_EASE_MS, chat, flow, hotbar, interface, scene};
use base::SoundEvent;
use base::{Arg, EntityId, TextRef, TilePos};
use client_state::{ChatLine, QuestNews};
use protocol::{ChatChannel, ClientMsg, EntityKind, TILE_SWITCH};
use std::time::Duration;
use tracing::error;
use web_time::Instant;
use winit::event_loop::ActiveEventLoop;

/// Tiles within which a revival item reaches a fallen player; the
/// server checks the same (`sim::actions::REVIVE_RADIUS`).
const REVIVE_REACH: u32 = 2;
/// Tiles from an NPC at which `Talk` is sent (the server allows 2).
const TALK_RADIUS: u32 = 2;
/// While a movement key is held the same destination is sent again at
/// most this often (a lost or refused order is not a stop).
const WASD_RESEND: Duration = Duration::from_millis(400);
/// A bag, shop or recipe slot pulses at most this long waiting for an answer.
const PENDING_MS: f64 = 1500.0;

impl App {
    /// The drawn zoom follows the wheel's over a few frames.
    fn ease_zoom(&mut self, now_ms: f64) {
        let dt = (now_ms - self.zoom_ms).clamp(0.0, 100.0);
        self.zoom_ms = now_ms;
        let k = 1.0 - (-dt / ZOOM_EASE_MS).exp();
        self.zoom += (self.zoom_goal - self.zoom) * k as f32;
        if (self.zoom_goal - self.zoom).abs() < 1e-4 {
            self.zoom = self.zoom_goal;
        }
    }

    pub(crate) fn game_frame(&mut self, event_loop: &ActiveEventLoop, now_ms: f64) {
        if let Some((code, args)) = self.state.take_error() {
            self.pending = None;
            error!(code = code.text_key(), "server error");
            let text = flow::error_text(&self.ui.text, code, &args);
            self.log.push(client_ui::LineKind::Error, text);
        }
        // The countdown of an announced restart.
        for text in self.flow.take_notices() {
            self.log.push(client_ui::LineKind::System, text);
        }
        self.content_text();
        self.chat_lines();
        self.ease_zoom(now_ms);
        if let Some(protocol::EntityName::Player(n)) = self.state.me().map(|m| m.name.clone()) {
            self.bar.select(n.as_str());
        }
        self.bar.sync(&self.state);
        self.walk_keys();
        if let Some(me) = self.state.me() {
            let (x, y) = me.visual_pos(now_ms);
            self.center = (x as f32, y as f32);
            let pos = me.pos;
            if self.destination == Some(pos) {
                self.destination = None;
            }
            self.state.forget_far_chunks(pos, protocol::KEEP_CHUNKS);
        }
        // A target that died or left the view is dropped at once, not kept
        // until the next click.
        if self
            .target
            .is_some_and(|t| self.state.entity(t).is_none_or(|e| e.health == 0))
        {
            self.target = None;
        }
        let cam = self.camera();
        self.floats.retain(|f| now_ms - f.born_ms < f.lasts_ms());
        let Some(render) = self.render.as_mut() else {
            return;
        };
        scene::build(
            &self.state,
            render.config(),
            &cam,
            now_ms,
            // Keys walk two tiles ahead: no marker for that.
            self.destination.filter(|_| !self.key_walking),
            self.target,
            (
                &self.graphics,
                self.graphics.is_night(self.state.day_fraction(now_ms)),
            ),
            (&mut self.items, &mut self.lights, &mut self.fog),
        );
        // The ground out of sight, softened by the renderer.
        render.set_sight(self.fog.mask().cloned());
        // Phase 10: the time of day tints the world; lights shine in it.
        render.set_lighting(client_render::Lighting {
            ambient: self.graphics.ambient(self.state.day_fraction(now_ms)),
            lights: self.lights.clone(),
        });
        let cfg = *render.config();
        let input = self.take_input(&cam, now_ms);
        self.sound.poll();
        let here = self.state.me().map(|e| e.pos);
        self.sound.music(here.and_then(|h| self.state.zone_at(h)));
        if !self.log.error_new.is_empty() {
            self.sound.play(SoundEvent::UiError, None, 0);
        } else if !self.log.system_new.is_empty() {
            self.sound.play(SoundEvent::SystemNotice, None, 0);
        }
        let mut model = interface::model(
            &self.state,
            &self.texts,
            &self.ui.text,
            &mut self.log,
            &interface::ModelInput {
                keys: &self.keys,
                gfx: &self.graphics,
                target: self.target,
                now_ms,
                bar: &self.bar,
                pending: self.pending.map(|(p, _)| p).unwrap_or_default(),
            },
        );
        model.sound = self.sound.model();
        model.overhead = interface::overhead(&self.state, &self.texts, self.target, (&cfg, &cam), now_ms);
        model.floats = interface::floats(&self.state, &self.floats, (&cfg, &cam), now_ms);
        let right = input.right_clicked;
        let out = self.ui.frame(&input, |f, hud| hud.draw(f, &model));
        if input.clicked && out.over_ui {
            self.sound.play(SoundEvent::UiClick, None, 0);
        }
        let world_click = std::mem::take(&mut self.pending_click) && !out.over_ui;
        if input.wheel != 0.0 && !out.over_ui {
            let k = 1.1f32.powf(input.wheel.clamp(-3.0, 3.0));
            self.zoom_goal = (self.zoom_goal * k).clamp(self.min_zoom(), MAX_ZOOM);
        }
        let world_right = right && !out.over_ui;
        self.after_ui(&input, &out);
        self.draw(&cam, true);
        for a in out.actions {
            self.ui_action(event_loop, a);
        }
        if let Some(r) = self.render.as_ref() {
            let tile = r.screen_to_tile(&cam, self.cursor.0, self.cursor.1);
            if world_click {
                self.click(tile);
            } else if world_right {
                self.right_click(tile);
            }
        }
    }

    /// Text pack, System messages, combat lines and a pending `Talk`;
    /// once per frame.
    fn content_text(&mut self) {
        let hash = self.state.content_hash();
        let _ = self.state.take_quests_changed();
        let _ = self.state.take_dialogue_changed();
        let bag = self.state.take_inventory_changed();
        let stats = self.state.take_stats_changed();
        let now_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        // The answer came (or never will): the pulse stops.
        let answered = bag || stats || self.state.take_shop_changed();
        if answered || self.pending.is_some_and(|(_, at)| now_ms - at > PENDING_MS) {
            self.pending = None;
        }
        if let Some(pack) = self.state.take_text_pack() {
            self.texts.received(pack, hash);
        }
        for (msg, window) in self.state.take_system() {
            let text = self.texts.show(&msg);
            // Routine messages share the System color, without a window.
            let kind = if window {
                client_ui::LineKind::System
            } else {
                client_ui::LineKind::Quest
            };
            self.log.push(kind, text);
        }
        let me = self.state.my_id();
        let now_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        for (on, emote) in self.state.take_emotes() {
            if let Some(name) = self.state.emote_name(emote) {
                self.floats.push(interface::Float {
                    on,
                    text: self.texts.show(&base::TextRef::new(name)),
                    kind: client_ui::FloatKind::Emote,
                    crit: false,
                    born_ms: now_ms,
                    icon: self
                        .state
                        .emote_icon(emote)
                        .and_then(|n| self.graphics.icon_uv(Some(n), "")),
                });
            }
        }
        // What came into the backpack rises over the player, one after the
        // other so several gains do not cover each other.
        if let Some(on) = me {
            for (i, (item, qty)) in self.state.take_gains().into_iter().enumerate() {
                let icon = self.state.item(item).and_then(|d| self.state.icon_name(d.icon));
                self.floats.push(interface::Float {
                    on,
                    text: format!("+{qty}"),
                    kind: client_ui::FloatKind::Gained,
                    crit: false,
                    born_ms: now_ms + 250.0 * i as f64,
                    icon: self.graphics.icon_uv(icon, "generic_item"),
                });
            }
        }
        let radius = self.state.view_radius();
        let here = self.state.me().map(|e| e.pos);
        // Support techniques land no blow; their own message
        // says who used one, heard where the user stands.
        for u in self.state.take_supports() {
            let Some(id) = self.state.technique_info(u.technique).map(|i| i.id.clone()) else {
                continue;
            };
            let from = match (here, self.state.entity(u.user)) {
                (Some(h), Some(a)) => Some(((a.pos.x - h.x) as f32, (a.pos.y - h.y) as f32)),
                _ => None,
            };
            self.sound.technique(&id, from, radius);
        }
        for c in self.state.take_combat() {
            let mine = Some(c.attacker) == me || Some(c.target) == me;
            let event = match c.damage {
                Some(_) if Some(c.target) == me => SoundEvent::CombatHurt,
                None => SoundEvent::CombatMiss,
                Some(_) if c.crit => SoundEvent::CombatCrit,
                Some(_) => SoundEvent::CombatHit,
            };
            let at = match (here, self.state.entity(c.target)) {
                (Some(h), Some(t)) => Some(((t.pos.x - h.x) as f32, (t.pos.y - h.y) as f32)),
                _ => None,
            };
            self.sound.play(event, at, radius);
            // The technique's own sound, where the attacker stands.
            if let Some(id) = c
                .technique
                .and_then(|t| self.state.technique_info(t))
                .map(|i| i.id.clone())
            {
                let from = match (here, self.state.entity(c.attacker)) {
                    (Some(h), Some(a)) => Some(((a.pos.x - h.x) as f32, (a.pos.y - h.y) as f32)),
                    _ => None,
                };
                self.sound.technique(&id, from, radius);
            }
            if mine {
                let kind = match c.damage {
                    None => client_ui::FloatKind::Miss,
                    Some(_) if Some(c.target) == me => client_ui::FloatKind::Taken,
                    Some(_) => client_ui::FloatKind::Dealt,
                };
                let text = c
                    .damage
                    .map_or_else(|| self.ui.text.t("ui.combat.miss"), |d| d.to_string());
                self.floats.push(interface::Float {
                    on: c.target,
                    text,
                    kind,
                    crit: c.crit,
                    born_ms: now_ms,
                    icon: None,
                });
            }
            if mine || Some(c.target) == self.target {
                let (a, t) = (
                    interface::entity_name(&self.state, &self.texts, c.attacker),
                    interface::entity_name(&self.state, &self.texts, c.target),
                );
                let amount = c.damage.map_or_else(
                    || "—".to_owned(),
                    |d| format!("{d}{}", if c.crit { "!" } else { "" }),
                );
                self.log
                    .push(client_ui::LineKind::Combat, format!("{a} > {t}: {amount}"));
            }
        }
        // Only what changed: the whole log lives in its window (L).
        for news in self.state.take_quest_news() {
            let name = |q| self.texts.show(&TextRef::new(q));
            let line = match &news {
                QuestNews::Started(q) => {
                    let key = self
                        .keys
                        .chord_of(client_ui::Action::OpenQuests)
                        .map_or_else(String::new, |c| interface::key_label(&c.key));
                    self.ui
                        .text
                        .t_args("ui.quests.started", &[Arg::Str(name(*q)), Arg::Str(key)])
                }
                QuestNews::Completed(q) => self.ui.text.t_args("ui.quests.completed", &[Arg::Str(name(*q))]),
                QuestNews::Progress { quest, objective } => {
                    let o = interface::objective_text(&self.state, &self.texts, &self.ui.text, objective);
                    self.ui
                        .text
                        .t_args("ui.quests.progress", &[Arg::Str(name(*quest)), Arg::Str(o)])
                }
            };
            // Started and completed earn the System window; progress only the log.
            let kind = match news {
                QuestNews::Progress { .. } => client_ui::LineKind::Quest,
                _ => client_ui::LineKind::System,
            };
            self.log.push(kind, line);
        }
        if let crate::text::Need::Request(locale) = self.texts.need(hash) {
            self.send(&ClientMsg::RequestTextPack { locale });
        }
        // An NPC whose shop was asked for from afar (phase 7).
        if let (Some(npc), Some(me)) = (self.shop_at, self.state.me()) {
            match self.state.entity(npc) {
                Some(e) if e.pos.distance(me.pos) <= TALK_RADIUS => {
                    self.send(&ClientMsg::OpenShop { target: npc });
                    self.shop_at = None;
                }
                Some(_) => {}
                None => self.shop_at = None,
            }
        }
        // A node clicked from afar: gather it on arrival (phase 8).
        if let (Some(node), Some(me)) = (self.gather_at, self.state.me()) {
            match self.state.entity(node) {
                Some(e) if e.pos.distance(me.pos) <= TALK_RADIUS => {
                    self.send(&ClientMsg::Gather { target: node });
                    self.gather_at = None;
                }
                Some(_) => {}
                None => self.gather_at = None,
            }
        }
        // A brazier clicked from afar: switched on arrival (phase 11).
        if let (Some(pos), Some(me)) = (self.use_at, self.state.me())
            && me.pos.distance(pos) <= TALK_RADIUS
        {
            self.send(&ClientMsg::UseTile { pos });
            self.use_at = None;
        }
        if let (Some(npc), Some(me)) = (self.talk_to, self.state.me()) {
            match self.state.entity(npc) {
                Some(e) if e.pos.distance(me.pos) <= TALK_RADIUS => {
                    self.send(&ClientMsg::Talk { target: npc });
                    self.talk_to = None;
                }
                Some(_) => {}
                None => self.talk_to = None,
            }
        }
    }

    /// Chat lines that arrived: names and text are the players' own, only
    /// the frame around them is translated.
    fn chat_lines(&mut self) {
        let me = match self.state.me().map(|e| &e.name) {
            Some(protocol::EntityName::Player(n)) => Some(n.to_string()),
            _ => None,
        };
        for ChatLine { channel, from, text } in self.state.take_chat() {
            let (key, name) = match channel {
                ChatChannel::Local => ("ui.chat.local", from.to_string()),
                ChatChannel::Whisper(to) if me.as_deref() == Some(from.as_str()) => {
                    ("ui.chat.whisper_to", to.to_string())
                }
                ChatChannel::Whisper(_) => ("ui.chat.whisper_from", from.to_string()),
            };
            let line = self
                .ui
                .text
                .t_args(key, &[Arg::Str(name), Arg::Str(text.to_string())]);
            self.log.push(client_ui::LineKind::Chat, line);
        }
    }

    /// A technique of the hotbar on the target (or a dialogue option while
    /// a dialogue is open).
    fn hotbar(&mut self, i: u8) {
        if self.state.dialogue().is_some() {
            self.send(&ClientMsg::ChooseOption { option: u16::from(i) });
            return;
        }
        match interface::hotbar_entry(&self.state, &self.bar, usize::from(i)) {
            Some(interface::HotbarEntry::Technique(t)) => {
                let now_ms = self.started.elapsed().as_secs_f64() * 1000.0;
                let support = self.state.technique_info(t).map(|i| {
                    (
                        i.heal_permille > 0,
                        i.revive_permille > 0,
                        i.range,
                        i.buff_ticks > 0,
                    )
                });
                let target = match support {
                    // A buff (the vow) is always on oneself.
                    Some((_, _, _, true)) => self.state.me().map(|m| m.id),
                    // A heal goes to the player targeted, else oneself;
                    // a revival to the fallen player targeted, else the nearest.
                    Some((true, _, _, _)) => self
                        .target
                        .filter(|id| {
                            self.state
                                .entity(*id)
                                .is_some_and(|e| e.kind == EntityKind::Player && e.health > 0)
                        })
                        .or_else(|| self.state.me().map(|m| m.id)),
                    Some((_, true, range, _)) => {
                        let picked = self.target.filter(|id| {
                            self.state
                                .entity(*id)
                                .is_some_and(|e| e.kind == EntityKind::Player && e.health == 0)
                        });
                        match picked.or_else(|| self.nearest_fallen(u32::from(range))) {
                            Some(id) => Some(id),
                            None => {
                                let text = self.ui.text.t("ui.player.none_fallen");
                                return self.log.push(client_ui::LineKind::Error, text);
                            }
                        }
                    }
                    _ => {
                        let alive = self
                            .target
                            .is_some_and(|id| self.state.entity(id).is_some_and(|e| e.health > 0));
                        self.target.filter(|_| alive)
                    }
                };
                let Some(target) = target else {
                    let text = self.ui.text.t("ui.technique.no_target");
                    return self.log.push(client_ui::LineKind::Error, text);
                };
                // Refused here, without a round trip, when it cannot work.
                if self.state.cooldown_left_ms(t, now_ms) > 0.0 {
                    return;
                }
                if !self.state.weapon_fits(t) {
                    let text = self.ui.text.t("ui.technique.wrong_weapon");
                    return self.log.push(client_ui::LineKind::Error, text);
                }
                if let (Some(s), Some(info)) = (self.state.stats(), self.state.technique_info(t)) {
                    let missing = if s.vigor < info.vigor {
                        Some("ui.technique.no_vigor")
                    } else if s.mp < info.mp {
                        Some("ui.technique.no_mp")
                    } else {
                        None
                    };
                    if let Some(key) = missing {
                        let text = self.ui.text.t(key);
                        return self.log.push(client_ui::LineKind::Error, text);
                    }
                }
                self.state.note_technique_used(t, now_ms);
                self.send(&ClientMsg::UseTechnique { technique: t, target });
            }
            // A consumable in the bar is used straight away.
            Some(interface::HotbarEntry::Item(Some(slot), _)) => self.use_item(slot),
            Some(interface::HotbarEntry::Item(None, _)) => {
                let text = self.ui.text.t("ui.hotbar.none_left");
                self.log.push(client_ui::LineKind::Error, text);
            }
            None => {}
        }
    }

    /// An action that the server answers with a new backpack or sheet.
    fn mark(&mut self, p: client_ui::Pending) {
        let now_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        self.pending = Some((p, now_ms));
    }

    fn mark_bag(&mut self, slot: u16) {
        self.mark(client_ui::Pending {
            bag: Some(slot),
            ..Default::default()
        });
    }

    /// Something dropped on hotbar slot `slot`.
    fn hotbar_drop(&mut self, slot: u8, from: client_ui::DragSource) {
        use client_ui::DragSource as D;
        let slot = usize::from(slot);
        match from {
            D::Hotbar(j) => self.bar.swap(usize::from(j), slot),
            D::Technique(t) => self.bar.place(slot, hotbar::Bind::Technique(t.get())),
            D::Bag(b) => match interface::bag_bind(&self.state, b) {
                Some(bind) => self.bar.place(slot, bind),
                None => {
                    let text = self.ui.text.t("ui.hotbar.not_usable");
                    self.log.push(client_ui::LineKind::Error, text);
                }
            },
        }
    }

    fn ui_action(&mut self, event_loop: &ActiveEventLoop, a: client_ui::UiAction) {
        use client_ui::UiAction as A;
        let stats = self.state.stats().cloned();
        match a {
            A::Hotbar(i) => self.hotbar(i),
            A::HotbarDrop { slot, from } => self.hotbar_drop(slot, from),
            A::HotbarClear(i) => self.bar.clear(usize::from(i)),
            A::SoundStep { slot, up } => self.sound.step(slot, up),
            A::SoundMute => self.sound.toggle_mute(),
            A::SoundTest => self.sound.play(SoundEvent::CombatHit, None, 0),
            A::HotbarAdd(t) => match self.bar.free() {
                Some(free) => self.bar.place(free, hotbar::Bind::Technique(t.get())),
                None => {
                    let text = self.ui.text.t("ui.hotbar.full");
                    self.log.push(client_ui::LineKind::Error, text);
                }
            },
            A::ChooseOption(n) => self.send(&ClientMsg::ChooseOption { option: n }),
            A::ChooseClass(i) => {
                if let Some(class) = stats.and_then(|s| s.classes_available.get(i).copied()) {
                    self.send(&ClientMsg::ChooseClass { class });
                }
            }
            A::AddPoint(attr) => {
                let mut points = [0u16; 6];
                if let Some(p) = points.get_mut(attr) {
                    *p = 1;
                    self.send(&ClientMsg::AllocatePoints { points });
                }
            }
            A::Equip(slot) => {
                self.mark_bag(slot);
                self.send(&ClientMsg::Equip { slot });
            }
            A::Appraise(slot) => {
                self.mark_bag(slot);
                self.send(&ClientMsg::Appraise { slot });
            }
            A::SelfRepair(slot) => {
                self.mark_bag(slot);
                self.send(&ClientMsg::SelfRepair { slot });
            }
            A::UseItem(slot) => self.use_item(slot),
            A::Unequip(i) => {
                let slot = match i {
                    0 => protocol::EquipSlot::Weapon,
                    2 => protocol::EquipSlot::Offhand,
                    _ => protocol::EquipSlot::Armor,
                };
                self.send(&ClientMsg::Unequip { slot });
            }
            A::GiveUp => self.send(&ClientMsg::GiveUp),
            // Phase 7: the System line says the price paid.
            A::Buy { index, qty } => {
                self.mark(client_ui::Pending {
                    shop: Some(index),
                    ..Default::default()
                });
                self.send(&ClientMsg::Buy { index, qty });
            }
            A::Sell { slot, qty } => {
                self.mark_bag(slot);
                self.send(&ClientMsg::Sell { slot, qty });
            }
            A::Repair(slot) => {
                self.mark_bag(slot);
                self.send(&ClientMsg::Repair { slot });
            }
            A::ShopAppraise(slot) => {
                self.mark_bag(slot);
                self.send(&ClientMsg::ShopAppraise { slot });
            }
            A::MoveItem { from, to } => self.send(&ClientMsg::MoveItem { from, to }),
            A::CloseShop => self.send(&ClientMsg::CloseShop),
            A::Craft(recipe) => {
                self.mark(client_ui::Pending {
                    recipe: Some(recipe),
                    ..Default::default()
                });
                self.send(&ClientMsg::Craft { recipe });
            }
            A::Gather(target) => self.send(&ClientMsg::Gather { target }),
            // The trade window. The server checks every piece.
            A::TradeAdd(slot) => self.trade_add(slot),
            A::TradeOffer { items, coins } => self.send(&ClientMsg::TradeOffer { items, coins }),
            A::TradeConfirm => self.send(&ClientMsg::TradeConfirm),
            A::TradeCancel => self.send(&ClientMsg::TradeCancel),
            A::TradeAccept => {
                // Asking back is the answer to a trade request.
                if let Some(target) = self.state.trade().map(|t| t.with) {
                    self.send(&ClientMsg::TradeRequest { target });
                }
            }
            A::Player { target, name, act } => self.player_act(target, &name, act),
            A::LeaveParty => self.send(&ClientMsg::LeaveParty),
            A::Emote(emote) => self.send(&ClientMsg::Emote { emote }),
            A::AnswerInvite(true) => self.social(chat::Social::Accept),
            A::AnswerInvite(false) => self.social(chat::Social::Decline),
            A::SendChat(text) => self.chat(&text),
            A::Quit => event_loop.exit(),
            _ => {}
        }
    }

    /// A backpack slot joins my trade offer, the whole stack; a slot
    /// already offered stays as it is.
    fn trade_add(&mut self, slot: u16) {
        let Some(t) = self.state.trade().filter(|t| t.open) else {
            return;
        };
        if t.mine.iter().any(|(s, _)| *s == slot) {
            return;
        }
        let Some(qty) = self
            .state
            .inventory()
            .and_then(|i| i.slots.get(usize::from(slot)))
            .and_then(|s| s.as_ref())
            .map(|s| s.qty.max(1))
        else {
            return;
        };
        let mut items = t.mine.clone();
        items.push((slot, qty));
        let coins = t.mine_coins;
        self.send(&ClientMsg::TradeOffer { items, coins });
    }

    /// A consumable of the backpack used (click or hotbar). A revival item
    /// goes to the nearest fallen player beside the character.
    fn use_item(&mut self, slot: u16) {
        if self.is_revival(slot) {
            match self.nearest_fallen(REVIVE_REACH) {
                Some(target) => {
                    self.mark_bag(slot);
                    self.send(&ClientMsg::UseItemOn { slot, target });
                }
                None => {
                    let text = self.ui.text.t("ui.player.none_fallen");
                    self.log.push(client_ui::LineKind::Error, text);
                }
            }
            return;
        }
        self.mark_bag(slot);
        let item = self
            .state
            .inventory()
            .and_then(|i| i.slots.get(usize::from(slot)))
            .and_then(|s| s.as_ref())
            .map(|s| s.item);
        if let Some(item) = item {
            let now_ms = self.started.elapsed().as_secs_f64() * 1000.0;
            self.state.note_item_used(item, now_ms);
        }
        self.send(&ClientMsg::UseItem { slot });
    }

    /// The nearest fallen player (not oneself) within `reach` tiles.
    fn nearest_fallen(&self, reach: u32) -> Option<EntityId> {
        let (id, pos) = self.state.me().map(|m| (m.id, m.pos))?;
        self.state
            .entities()
            .filter(|e| e.kind == EntityKind::Player && e.id != id && e.health == 0)
            .map(|e| (e.pos.distance(pos), e.id))
            .filter(|(d, _)| *d <= reach)
            .min()
            .map(|(_, id)| id)
    }

    /// A known technique of this kind (heal or revival) ready to go now:
    /// off cooldown, with the MP and vigor it costs.
    fn ready_support(&self, revive: bool) -> Option<base::TechniqueId> {
        let now_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        let s = self.state.stats()?;
        s.techniques
            .iter()
            .map(|(t, _)| *t)
            .filter(|t| {
                self.state.technique_info(*t).is_some_and(|i| {
                    let kind = if revive {
                        i.revive_permille > 0
                    } else {
                        i.heal_permille > 0
                    };
                    kind && s.mp >= i.mp && s.vigor >= i.vigor
                })
            })
            .find(|t| self.state.cooldown_left_ms(*t, now_ms) <= 0.0)
    }

    /// The backpack slot holds a revival item.
    fn is_revival(&self, slot: u16) -> bool {
        self.state
            .inventory()
            .and_then(|i| i.slots.get(usize::from(slot)))
            .and_then(|s| s.as_ref())
            .and_then(|s| self.state.item(s.item))
            .and_then(|d| d.use_effect)
            .is_some_and(|u| u.revive_permille > 0)
    }

    /// An option of the player menu: the same messages the
    /// chat commands send.
    fn player_act(&mut self, target: EntityId, name: &str, act: client_ui::PlayerAct) {
        use client_ui::PlayerAct as P;
        match act {
            P::Trade => self.send(&ClientMsg::TradeRequest { target }),
            P::Invite => self.send(&ClientMsg::PartyInvite { target }),
            P::Lead => self.send(&ClientMsg::PartyLead { target }),
            P::Kick => self.send(&ClientMsg::PartyKick { target }),
            P::Whisper => {
                // The chat opens with the whisper started: only the text is typed.
                self.ui.hud.chat_open = true;
                self.ui.hud.chat_draft = format!("/w {name} ");
            }
            P::Heal => match self.ready_support(false) {
                Some(technique) => self.send(&ClientMsg::UseTechnique { technique, target }),
                None => {
                    let text = self.ui.text.t("ui.player.no_heal");
                    self.log.push(client_ui::LineKind::Error, text);
                }
            },
            // The technique when it is ready, else an item.
            P::Revive if self.ready_support(true).is_some() => {
                if let Some(technique) = self.ready_support(true) {
                    self.send(&ClientMsg::UseTechnique { technique, target });
                }
            }
            P::Revive => {
                let slot = self
                    .state
                    .inventory()
                    .map_or(0, |i| i.slots.len())
                    .try_into()
                    .unwrap_or(u16::MAX);
                match (0..slot).find(|s| self.is_revival(*s)) {
                    Some(slot) => {
                        self.mark_bag(slot);
                        self.send(&ClientMsg::UseItemOn { slot, target });
                    }
                    None => {
                        let text = self.ui.text.t("ui.player.no_revive");
                        self.log.push(client_ui::LineKind::Error, text);
                    }
                }
            }
            P::Report => match base::CharacterName::try_from(name) {
                Ok(name) => self.send(&ClientMsg::ReportChat { name }),
                Err(_) => {
                    let text = self
                        .ui
                        .text
                        .t_args("ui.social.not_here", &[Arg::Str(name.into())]);
                    self.log.push(client_ui::LineKind::Error, text);
                }
            },
        }
    }

    /// A line typed in the chat: a message, or a command (`/w`, `/mute`...).
    /// What the server answers comes back as `ChatMessage` or `System`.
    fn chat(&mut self, line: &str) {
        if line.trim().is_empty() {
            return;
        }
        match chat::parse(line) {
            chat::Typed::Send(msg) => self.send(&msg),
            chat::Typed::Usage(usage) => {
                let text = self.ui.text.t_args("ui.chat.usage", &[Arg::Str(usage.into())]);
                self.log.push(client_ui::LineKind::Error, text);
            }
            chat::Typed::Social(cmd) => self.social(cmd),
            chat::Typed::Emote(name) => self.emote(name),
            chat::Typed::Unknown(name) => {
                let text = self.ui.text.t_args("ui.chat.unknown_command", &[Arg::Str(name)]);
                self.log.push(client_ui::LineKind::Error, text);
            }
        }
    }

    /// The player entity called `name`, as the client sees it.
    fn player_named(&self, name: &base::CharacterName) -> Option<EntityId> {
        self.state
            .entities()
            .filter(|e| e.kind == EntityKind::Player)
            .find(|e| {
                interface::entity_name(&self.state, &self.texts, e.id).eq_ignore_ascii_case(name.as_str())
            })
            .map(|e| e.id)
    }

    /// A group or trade command (phase 9). The server checks everything.
    fn social(&mut self, cmd: chat::Social) {
        use chat::Social as S;
        match cmd {
            S::Invite(n) => {
                if let Some(target) = self.named(&n) {
                    self.send(&ClientMsg::PartyInvite { target });
                }
            }
            S::PartyKick(n) => {
                if let Some(target) = self.named(&n) {
                    self.send(&ClientMsg::PartyKick { target });
                }
            }
            S::PartyLead(n) => {
                if let Some(target) = self.named(&n) {
                    self.send(&ClientMsg::PartyLead { target });
                }
            }
            S::Trade(n) => {
                if let Some(target) = self.named(&n) {
                    self.send(&ClientMsg::TradeRequest { target });
                }
            }
            S::Accept => match self.state.invite() {
                Some((_, protocol::InviteKind::Party)) => {
                    self.send(&ClientMsg::AnswerInvite { accept: true });
                    self.state.answer_invite();
                }
                // Asking back is the answer to a trade request.
                Some((target, protocol::InviteKind::Trade)) => {
                    self.send(&ClientMsg::TradeRequest { target });
                    self.state.answer_invite();
                }
                None => {}
            },
            S::Decline => match self.state.invite() {
                Some((_, protocol::InviteKind::Party)) => {
                    self.send(&ClientMsg::AnswerInvite { accept: false });
                    self.state.answer_invite();
                }
                Some((_, protocol::InviteKind::Trade)) => {
                    self.send(&ClientMsg::TradeCancel);
                    self.state.answer_invite();
                }
                None => {}
            },
            S::Leave => self.send(&ClientMsg::LeaveParty),
            S::Offer { items, coins } => self.send(&ClientMsg::TradeOffer { items, coins }),
            S::Confirm => self.send(&ClientMsg::TradeConfirm),
            S::Cancel => self.send(&ClientMsg::TradeCancel),
        }
    }

    /// `/emote NAME` makes the gesture whose name (in the player's language)
    /// matches; with no name it lists them.
    fn emote(&mut self, name: Option<String>) {
        let names: Vec<(base::EmoteId, String)> = self
            .state
            .emotes()
            .map(|(id, n)| (id, self.texts.show(&base::TextRef::new(n))))
            .collect();
        match name {
            Some(want) => match names.iter().find(|(_, n)| n.eq_ignore_ascii_case(&want)) {
                Some((emote, _)) => self.send(&ClientMsg::Emote { emote: *emote }),
                None => {
                    let text = self.ui.text.t_args("ui.emote.unknown", &[Arg::Str(want)]);
                    self.log.push(client_ui::LineKind::Error, text);
                }
            },
            None => {
                let list = names.into_iter().map(|(_, n)| n).collect::<Vec<_>>().join(", ");
                let text = self.ui.text.t_args("ui.emote.list", &[Arg::Str(list)]);
                self.log.push(client_ui::LineKind::System, text);
            }
        }
    }

    /// A player by name; a line in the log when nobody near has it.
    fn named(&mut self, n: &base::CharacterName) -> Option<EntityId> {
        let id = self.player_named(n);
        if id.is_none() {
            let text = self
                .ui
                .text
                .t_args("ui.social.not_here", &[Arg::Str(n.to_string())]);
            self.log.push(client_ui::LineKind::Error, text);
        }
        id
    }

    /// One step of WASD movement, in screen directions: up on the screen
    /// is the tile to the north-west in the isometric view.
    /// Walking somewhere else drops what was waiting for the arrival, so a
    /// dialogue, shop or gathering does not open later, out of the blue.
    fn cancel_intents(&mut self) {
        self.talk_to = None;
        self.shop_at = None;
        self.gather_at = None;
        self.use_at = None;
    }

    /// Held movement keys walk two tiles ahead, sent again as each step
    /// lands: the server keeps its own pace (one tile per `STEP_TICKS`)
    /// with no gap between steps. One tile at a time on the client's clock
    /// raced the server's ticks, so some steps took 250 ms and the next
    /// 200 ms, and the walk seemed to lurch forward. Letting go stops.
    fn walk_keys(&mut self) {
        let [up, down, left, right] = self.held;
        let (mut dx, mut dy) = (0i32, 0i32);
        for (on, (x, y)) in [(up, (-1, -1)), (down, (1, 1)), (left, (-1, 1)), (right, (1, -1))] {
            if on {
                dx += x;
                dy += y;
            }
        }
        let (dx, dy) = (dx.clamp(-1, 1), dy.clamp(-1, 1));
        if (dx, dy) == (0, 0) || self.typing {
            if std::mem::take(&mut self.key_walking) {
                self.send(&ClientMsg::Stop);
                self.destination = None;
            }
            return;
        }
        let Some(me) = self.state.me() else { return };
        let one = TilePos::new(me.pos.x + dx, me.pos.y + dy);
        let two = TilePos::new(me.pos.x + 2 * dx, me.pos.y + 2 * dy);
        // A wall right behind the next tile: only the next tile, or the
        // planner would walk around it or refuse the whole order.
        let to = if !self.state.blocks(one) && !self.state.blocks(two) {
            two
        } else {
            one
        };
        if self.key_walking && self.destination == Some(to) && self.last_step.elapsed() < WASD_RESEND {
            return;
        }
        self.cancel_intents();
        self.send(&ClientMsg::Move { destination: to });
        self.destination = Some(to);
        self.last_step = Instant::now();
        self.key_walking = true;
    }

    pub(crate) fn action(&mut self, _event_loop: &ActiveEventLoop, a: client_ui::Action, pressed: bool) {
        use client_ui::Action as K;
        let held = match a {
            K::MoveUp => Some(0),
            K::MoveDown => Some(1),
            K::MoveLeft => Some(2),
            K::MoveRight => Some(3),
            _ => None,
        };
        if let Some(i) = held {
            self.held[i] = pressed;
            return;
        }
        if !pressed {
            return;
        }
        match a {
            K::Stop => {
                self.cancel_intents();
                self.send(&ClientMsg::Stop);
                self.destination = None;
            }
            K::Hotbar(i) => self.hotbar(i),
            K::ChatOpen => self.ui.hud.chat_open = true,
            K::OpenInventory => self.ui.hud.toggle_inventory(),
            K::OpenSheet => self.ui.hud.toggle_sheet(),
            K::OpenQuests => self.ui.hud.toggle_quests(),
            K::OpenCrafting => self.ui.hud.toggle_crafting(),
            K::OpenTechniques => self.ui.hud.toggle_techniques(),
            K::OpenHelp => self.ui.hud.toggle_help(),
            K::OpenOptions => self.ui.hud.toggle_options(),
            K::ToggleMute => self.sound.toggle_mute(),
            K::Screenshot => self.screenshot(),
            K::TargetClear | K::Menu => {
                // What the mouse carries, the shop, then the windows, the
                // dialogue and the target.
                if self.ui.hud.dragging() {
                    self.ui.hud.close_top();
                } else if self.state.shop().is_some() {
                    self.send(&ClientMsg::CloseShop);
                } else if !self.ui.hud.close_top() {
                    if self.state.dialogue().is_some() {
                        self.send(&ClientMsg::CloseDialogue);
                    } else {
                        self.target = None;
                    }
                }
            }
            K::TargetNext => self.target = self.next_monster(),
            K::Interact => {
                if let Some(npc) = self.nearest(EntityKind::Npc) {
                    self.talk_to = Some(npc);
                }
            }
            _ => {}
        }
    }

    /// Right click: opens the shop of an NPC that keeps one (phase 7; the
    /// server answers `NoShop` when it keeps none), or the menu of another
    /// player (trade, group, whisper).
    fn right_click(&mut self, tile: TilePos) {
        self.cancel_intents();
        let player = self.entity_under(tile, &[EntityKind::Player]);
        if let Some(target) = player {
            self.ui.hud.player_menu = Some(client_ui::PlayerMenu {
                target,
                name: interface::entity_name(&self.state, &self.texts, target),
                at: self.cursor,
                fallen: self.state.entity(target).is_some_and(|e| e.health == 0),
            });
            return;
        }
        let Some(target) = self.entity_under(tile, &[EntityKind::Npc]) else {
            return;
        };
        let tile = self.state.entity(target).map_or(tile, |e| e.pos);
        let near = self
            .state
            .me()
            .is_some_and(|me| me.pos.distance(tile) <= TALK_RADIUS);
        if near {
            self.send(&ClientMsg::OpenShop { target });
        } else {
            self.shop_at = Some(target);
            self.send(&ClientMsg::Move { destination: tile });
            self.destination = Some(tile);
        }
    }

    /// The creature drawn under a click on `tile`, of one of `kinds` (not
    /// oneself). A 64×64 creature's body rises above the tile of its feet,
    /// so a click on the body lands on a tile behind it: the clicked tile
    /// counts first, then the tiles in front of it, nearest first.
    fn entity_under(&self, tile: TilePos, kinds: &[EntityKind]) -> Option<EntityId> {
        const BODY: [(i32, i32); 6] = [(0, 0), (1, 1), (1, 0), (0, 1), (2, 1), (1, 2)];
        let me = self.state.my_id();
        BODY.iter().find_map(|(dx, dy)| {
            let at = TilePos::new(tile.x + dx, tile.y + dy);
            self.state
                .entities()
                .find(|e| kinds.contains(&e.kind) && e.pos == at && Some(e.id) != me)
                .map(|e| e.id)
        })
    }

    /// Tab: the nearest living monster after the current target, so
    /// pressing it again walks through the ones around.
    fn next_monster(&self) -> Option<EntityId> {
        let me = self.state.me()?;
        let mut near: Vec<_> = self
            .state
            .entities()
            .filter(|e| e.kind == EntityKind::Monster && e.health > 0)
            .map(|e| (e.pos.distance(me.pos), e.id))
            .collect();
        near.sort_by_key(|(d, id)| (*d, id.get()));
        let next = self
            .target
            .and_then(|t| near.iter().position(|(_, id)| *id == t))
            .map_or(0, |i| i + 1);
        near.get(next).or(near.first()).map(|(_, id)| *id)
    }

    fn nearest(&self, kind: EntityKind) -> Option<EntityId> {
        let me = self.state.me()?;
        self.state
            .entities()
            .filter(|e| e.kind == kind && e.id != me.id)
            .min_by_key(|e| (e.pos.distance(me.pos), e.id.get()))
            .map(|e| e.id)
    }

    /// Left click: attack a monster (or player), talk to an NPC (walking
    /// next to it first), or walk there.
    fn click(&mut self, tile: TilePos) {
        self.cancel_intents();
        // The server walks the character in reach and swings on its own.
        let foe = self.entity_under(tile, &[EntityKind::Monster, EntityKind::Player]);
        if let Some(target) = foe {
            self.target = Some(target);
            self.send(&ClientMsg::Attack { target });
            return;
        }
        // A resource node is gathered where it stands (phase 8).
        let node = self
            .state
            .entities()
            .find(|e| e.kind == EntityKind::Node && e.pos == tile)
            .map(|e| e.id);
        if let Some(target) = node {
            let near = self
                .state
                .me()
                .is_some_and(|me| me.pos.distance(tile) <= TALK_RADIUS);
            if near {
                self.send(&ClientMsg::Gather { target });
            } else {
                self.gather_at = Some(target);
                self.send(&ClientMsg::Move { destination: tile });
                self.destination = Some(tile);
            }
            return;
        }
        // A tile that switches (a brazier): lit or put out (phase 11).
        if self.state.tile(tile).is_some_and(|w| w & TILE_SWITCH != 0) {
            let near = self
                .state
                .me()
                .is_some_and(|me| me.pos.distance(tile) <= TALK_RADIUS);
            if near {
                self.send(&ClientMsg::UseTile { pos: tile });
            } else {
                self.use_at = Some(tile);
                self.send(&ClientMsg::Move { destination: tile });
                self.destination = Some(tile);
            }
            return;
        }
        let npc = self.entity_under(tile, &[EntityKind::Npc]);
        let tile = npc.and_then(|n| self.state.entity(n)).map_or(tile, |e| e.pos);
        self.talk_to = npc;
        let near = self
            .state
            .me()
            .is_some_and(|me| me.pos.distance(tile) <= TALK_RADIUS);
        if npc.is_some() && near {
            return; // `content_text` sends `Talk` this frame.
        }
        self.send(&ClientMsg::Move { destination: tile });
        self.destination = Some(tile);
    }
}
