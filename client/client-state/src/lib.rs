//! Mirror of what the server says, plus interpolation for smooth movement.
//! Decides no rule: does not predict movement nor validate anything.
//!
//!

use base::{
    Arg, CharacterName, ChatText, Direction, EntityId, ItemId, Locale, Look, STEP_TICKS, TextRef, TilePos,
    WIDE_STEP_TICKS,
};
use protocol::{
    AppearanceInfo, CHUNK_SIZE, ChatChannel, ChunkTiles, CombatEvent, Entity, EntityKind, EntityName,
    ErrorCode, ItemInfo, ItemSlot, NamedInfo, ObjectiveView, QuestEntry, QuestMarker, QuestState, ServerMsg,
    StatsInfo, TILE_BLOCKS, TechniqueInfo,
};
use std::collections::BTreeMap;

/// Time one straight walked tile takes on screen (the server walks one tile
/// every 4 ticks at 20 Hz; 7 along the screen's horizontal).
pub const STEP_MS: f64 = 200.0;
/// One server tick (20 Hz), for cooldowns counted in ticks.
pub const TICK_MS: f64 = 50.0;
/// Guess of the global cooldown that follows any technique, until the server tells.
const GLOBAL_GUESS_MS: f64 = 500.0;

#[derive(Clone, Debug, PartialEq)]
pub struct VisibleEntity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub name: EntityName,
    pub appearance: u16,
    pub direction: Direction,
    /// HP in hundredths, weapon and armor layers, level (phase 4).
    pub health: u8,
    pub equipment: (u16, u16, u16),
    /// Protocol 22: `ItemId` + 1 of the item whose light shines here.
    pub light: u32,
    pub level: u8,
    /// Players: positions in the creation lists (phase 5).
    pub look: Look,
    /// Last position the server sent.
    pub pos: TilePos,
    /// Visual position when the current step started, in tiles.
    from: (f64, f64),
    step_start_ms: f64,
    /// How long the current step is drawn for (see `move_to`).
    step_ms: f64,
}

impl VisibleEntity {
    fn new(e: Entity, now_ms: f64) -> Self {
        let from = (f64::from(e.pos.x), f64::from(e.pos.y));
        VisibleEntity {
            id: e.id,
            kind: e.kind,
            name: e.name,
            appearance: e.appearance,
            direction: e.direction,
            health: e.health,
            equipment: e.equipment,
            light: e.light,
            level: e.level,
            look: e.look,
            pos: e.pos,
            from,
            step_start_ms: now_ms,
            step_ms: STEP_MS,
        }
    }

    /// Continuous position in tiles: from where it was drawn toward `pos`.
    pub fn visual_pos(&self, now_ms: f64) -> (f64, f64) {
        let t = self.step_progress(now_ms);
        let (tx, ty) = (f64::from(self.pos.x), f64::from(self.pos.y));
        (
            self.from.0 + (tx - self.from.0) * t,
            self.from.1 + (ty - self.from.1) * t,
        )
    }

    /// How far the drawing is along the current step: 0 at its start, 1
    /// once it stands on `pos`.
    pub fn step_progress(&self, now_ms: f64) -> f64 {
        if self.step_ms <= 0.0 {
            return 1.0;
        }
        ((now_ms - self.step_start_ms) / self.step_ms).clamp(0.0, 1.0)
    }

    /// Still drawn between two tiles: the walk frames play (phase 11).
    pub fn moving(&self, now_ms: f64) -> bool {
        now_ms - self.step_start_ms < self.step_ms
    }

    fn move_to(&mut self, pos: TilePos, direction: Direction, now_ms: f64) {
        // Continue from where it is drawn now: no jump if a step arrives early.
        let here = self.visual_pos(now_ms);
        let far = (here.0 - f64::from(pos.x)).abs() > 2.0 || (here.1 - f64::from(pos.y)).abs() > 2.0;
        if far {
            // A jump (a respawn, a seam) is not a walk: it lands at once, and
            // no walk frames play while it stands there.
            self.from = (f64::from(pos.x), f64::from(pos.y));
            self.pos = pos;
            self.direction = direction;
            self.step_start_ms = now_ms;
            self.step_ms = 0.0;
            return;
        }
        self.from = here;
        // A step that arrives early finds the drawing still on the last one:
        // running both in one fixed time would sprint. The time follows the
        // distance in ticks, the server's own pace (`Direction::step_ticks`:
        // longer along the screen's horizontal), with a little catching up.
        let (dx, dy) = (f64::from(pos.x) - here.0, f64::from(pos.y) - here.1);
        let n = dx.abs().max(dy.abs());
        let wide = if dx * dy < 0.0 {
            dx.abs().min(dy.abs())
        } else {
            0.0
        };
        let ticks = wide * WIDE_STEP_TICKS as f64 + (n - wide) * STEP_TICKS as f64;
        let own = direction.step_ticks() as f64;
        let behind = ticks.max(own) / own;
        let speed = 1.0 + 0.25 * (behind - 1.0).min(1.0);
        self.step_ms = own * TICK_MS * behind / speed;
        self.pos = pos;
        self.direction = direction;
        self.step_start_ms = now_ms;
    }
}

/// A chat line as it arrived (phase 5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    pub channel: ChatChannel,
    pub from: CharacterName,
    pub text: ChatText,
}

/// The dialogue node on screen: text and numbered options (1 = first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenDialogue {
    pub npc: EntityId,
    pub text: TextRef,
    pub options: Vec<TextRef>,
}

/// A text pack as it arrived; the binary decodes it with `i18n`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivedTextPack {
    pub locale: Locale,
    pub content_hash: u64,
    pub data: Vec<u8>,
}

#[derive(Default)]
pub struct WorldState {
    me: Option<EntityId>,
    /// Phase 10: the world's clock at `at_ms` (seconds since midnight, day length).
    world_time: Option<(u32, u32, f64)>,
    /// Days since the world began, from the last `WorldTime`.
    world_day: u64,
    content_hash: u64,
    server_tick: u32,
    /// Protocol 18: how far the player sees (`Welcome`).
    view_radius: u16,
    entities: BTreeMap<EntityId, VisibleEntity>,
    chunks: BTreeMap<(i32, i32), ChunkTiles>,
    last_error: Option<(ErrorCode, Vec<Arg>)>,
    pongs: Vec<u32>,
    /// Place in the entry queue and estimate in seconds, until `Welcome`.
    queue: Option<(u32, u32)>,
    dialogue: Option<OpenDialogue>,
    /// The dialogue opened, moved to another node or closed since the last
    /// `take_dialogue_changed`.
    dialogue_changed: bool,
    text_pack: Option<ReceivedTextPack>,
    /// Text pack parts received so far: `(pack so far, next part, parts)`.
    pack_parts: Option<(ReceivedTextPack, u16, u16)>,
    /// Items of `Definitions`; `ItemId` is the position.
    items: Vec<ItemInfo>,
    inventory: Option<Inventory>,
    quests: Vec<QuestEntry>,
    /// Backpack or quest log arrived since the last `take_*_changed`.
    inventory_changed: bool,
    quests_changed: bool,
    /// What changed between quest logs, for the chat; the first log after
    /// login is only the baseline (`quests_seen`).
    quest_news: Vec<QuestNews>,
    quests_seen: bool,
    /// Phase 4: own status window, names of classes/techniques/
    /// proficiencies/titles, System messages and blows not yet shown.
    stats: Option<StatsInfo>,
    stats_changed: bool,
    classes: Vec<NamedInfo>,
    techniques: Vec<TechniqueInfo>,
    proficiencies: Vec<NamedInfo>,
    titles: Vec<NamedInfo>,
    /// What each title gives, by `TitleId` (protocol 17).
    title_bonuses: Vec<Vec<protocol::TitleBonusInfo>>,
    /// Protocol 8: sprite and icon names (`appearance`, visuals and `icon`
    /// fields are the position + 1).
    sprites: Vec<String>,
    icons: Vec<String>,
    /// `(text, opens a System window)`.
    system: Vec<(TextRef, bool)>,
    /// Phase 5: when the last `Stats` arrived (its cooldowns count from
    /// there) and the quest markers over NPCs.
    stats_at_ms: f64,
    /// Techniques just used, before the server's `Stats` shows their
    /// cooldown: `(technique, when, own cooldown in ms)`. The sweep starts
    /// at the key press instead of one round trip later; a `Stats` or an
    /// `Error` that arrives afterwards replaces the guess.
    used: Vec<(base::TechniqueId, f64, f64)>,
    /// Consumables just used, before the server's `Stats` shows their
    /// cooldown: `(item, when, milliseconds)`.
    items_used: Vec<(ItemId, f64, f64)>,
    markers: BTreeMap<EntityId, QuestMarker>,
    combat: Vec<CombatEvent>,
    /// Items with identity in the backpack: `(slot, id, hidden once appraised)`.
    instances: Vec<protocol::InstanceInfo>,
    /// The worn pieces (slot 0 weapon, 1 armor).
    worn: Vec<protocol::InstanceInfo>,
    /// Phase 5: chat lines not yet shown, and the creation lists (they
    /// arrive with `Session` and stay for drawing looks).
    chat: Vec<ChatLine>,
    appearances: AppearanceInfo,
    /// Phase 7: the open shop (`None` when closed) and whether it changed
    /// since the last frame.
    shop: Option<protocol::ShopView>,
    shop_changed: bool,
    /// Phase 8: recipes of `Definitions`; `RecipeId` is the position.
    recipes: Vec<protocol::RecipeInfo>,
    /// Phase 7: NPCs of the zone that keep a shop.
    shop_npcs: std::collections::BTreeSet<EntityId>,
    /// Phase 9: the player's group and trade, and the invites not answered
    /// yet (oldest first).
    party: Option<protocol::PartyView>,
    trade: Option<protocol::TradeView>,
    invites: Vec<(EntityId, protocol::InviteKind)>,
    /// Phase 9: emote names of `Definitions` (`EmoteId` is the position) and
    /// the gestures not shown yet.
    emotes: Vec<base::TextId>,
    /// Protocol 20: icon number of each emote (0: none).
    emote_icons: Vec<u16>,
    emote_events: Vec<(EntityId, base::EmoteId)>,
    /// Items that came into the backpack since the last call (gathered,
    /// crafted, bought, looted): what the player sees rise over the head.
    gains: Vec<(ItemId, u32)>,
    /// Protocol 23: the zones of the world, for zone music.
    zones: Vec<protocol::ZoneInfo>,
    /// Protocol 23: support techniques used and not yet heard.
    supports: Vec<protocol::TechniqueUse>,
}

/// A change in the quest log worth a line in the chat. Quests are told
/// apart by their name (the log carries no quest id).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuestNews {
    Started(base::TextId),
    /// An objective moved forward (one more item up to the amount needed,
    /// or the talk done) in an active quest.
    Progress {
        quest: base::TextId,
        objective: ObjectiveView,
    },
    Completed(base::TextId),
}

/// The changes from `old` to `new`. Items leaving the backpack (handing
/// them over) are no progress.
pub fn quest_news(old: &[QuestEntry], new: &[QuestEntry]) -> Vec<QuestNews> {
    let mut news = Vec::new();
    for q in new {
        let Some(before) = old.iter().find(|o| o.name == q.name) else {
            news.push(match q.state {
                QuestState::Active => QuestNews::Started(q.name),
                QuestState::Completed => QuestNews::Completed(q.name),
            });
            continue;
        };
        match (before.state, q.state) {
            (QuestState::Active, QuestState::Completed) => news.push(QuestNews::Completed(q.name)),
            // Started again (repeatable).
            (QuestState::Completed, QuestState::Active) => news.push(QuestNews::Started(q.name)),
            (QuestState::Active, QuestState::Active) => {
                for (o, b) in q.objectives.iter().zip(&before.objectives) {
                    let forward = match (o, b) {
                        (ObjectiveView::Item { have, need, .. }, ObjectiveView::Item { have: was, .. }) => {
                            have > was && was < need
                        }
                        (ObjectiveView::Talk { done, .. }, ObjectiveView::Talk { done: was, .. }) => {
                            *done && !was
                        }
                        _ => false,
                    };
                    if forward {
                        news.push(QuestNews::Progress {
                            quest: q.name,
                            objective: o.clone(),
                        });
                    }
                }
            }
            (QuestState::Completed, QuestState::Completed) => {}
        }
    }
    news
}

/// The backpack: coins (copper) and one entry per slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inventory {
    pub coins: u64,
    pub slots: Vec<Option<ItemSlot>>,
}

/// What the player holds: stacks counted by item, pieces with identity by
/// their instance id (so moving a piece between the backpack and the hands
/// is no gain).
struct Holdings {
    stacks: BTreeMap<ItemId, u64>,
    pieces: BTreeMap<u64, Option<ItemId>>,
}

fn holdings(
    slots: &[Option<ItemSlot>],
    instances: &[protocol::InstanceInfo],
    worn: &[protocol::InstanceInfo],
) -> Holdings {
    let piece_slots: BTreeMap<u16, u64> = instances.iter().map(|i| (i.slot, i.id)).collect();
    let mut stacks = BTreeMap::new();
    let mut pieces = BTreeMap::new();
    for (i, s) in slots.iter().enumerate() {
        let Some(s) = s else { continue };
        match u16::try_from(i).ok().and_then(|i| piece_slots.get(&i)) {
            Some(id) => {
                pieces.insert(*id, Some(s.item));
            }
            None => *stacks.entry(s.item).or_insert(0) += u64::from(s.qty),
        }
    }
    for w in worn {
        pieces.entry(w.id).or_insert(None);
    }
    Holdings { stacks, pieces }
}

/// Stacks that grew and pieces that are new.
fn gained(before: &Holdings, after: &Holdings) -> Vec<(ItemId, u32)> {
    let mut out: Vec<(ItemId, u32)> = after
        .stacks
        .iter()
        .filter_map(|(item, n)| {
            let was = before.stacks.get(item).copied().unwrap_or(0);
            (*n > was).then(|| (*item, u32::try_from(n - was).unwrap_or(u32::MAX)))
        })
        .collect();
    for (id, item) in &after.pieces {
        if let (false, Some(item)) = (before.pieces.contains_key(id), item) {
            out.push((*item, 1));
        }
    }
    out
}

impl WorldState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, msg: ServerMsg, now_ms: f64) {
        match msg {
            ServerMsg::Welcome {
                your_id,
                tick,
                content_hash,
                view_radius,
                ..
            } => {
                self.view_radius = view_radius;
                self.me = Some(your_id);
                self.queue = None;
                self.server_tick = tick;
                self.content_hash = content_hash;
                self.entities.clear();
                self.dialogue = None;
            }
            ServerMsg::Chunk { cx, cy, tiles } => {
                self.chunks.insert((cx, cy), tiles);
            }
            ServerMsg::Snapshot { entities } => {
                // Phase 10: after crossing the seam the next zone sends the
                // whole view again; whoever was already on screen keeps
                // walking from where it is drawn, so nothing jumps.
                let mut old = std::mem::take(&mut self.entities);
                for e in entities {
                    let v = match old.remove(&e.id) {
                        Some(mut v) => {
                            let (pos, dir) = (e.pos, e.direction);
                            v.health = e.health;
                            v.equipment = e.equipment;
                            v.light = e.light;
                            v.level = e.level;
                            v.appearance = e.appearance;
                            v.look = e.look;
                            if v.pos != pos {
                                v.move_to(pos, dir, now_ms);
                            }
                            v
                        }
                        None => VisibleEntity::new(e, now_ms),
                    };
                    self.entities.insert(v.id, v);
                }
            }
            ServerMsg::WorldTime {
                secs_of_day,
                day_secs,
                day,
            } => {
                self.world_time = Some((secs_of_day, day_secs.max(1), now_ms));
                self.world_day = day;
            }
            // The login flow of the binary owns the restart notice.
            ServerMsg::Maintenance { .. } | ServerMsg::MaintenanceCancelled => {}
            ServerMsg::Diff(d) => {
                self.server_tick = d.tick;
                for id in d.left {
                    self.entities.remove(&id);
                }
                for e in d.entered {
                    self.entities.insert(e.id, VisibleEntity::new(e, now_ms));
                }
                for (id, pos, dir) in d.moved {
                    if let Some(e) = self.entities.get_mut(&id) {
                        e.move_to(pos, dir, now_ms);
                    }
                }
                for e in d.updated {
                    if let Some(v) = self.entities.get_mut(&e.id) {
                        v.health = e.health;
                        v.equipment = e.equipment;
                        v.light = e.light;
                        v.level = e.level;
                        v.appearance = e.appearance;
                        v.look = e.look;
                    }
                }
            }
            ServerMsg::Pong { n } => self.pongs.push(n),
            ServerMsg::Error { code, args } => {
                // The use was refused: no cooldown started.
                self.used.clear();
                self.items_used.clear();
                self.last_error = Some((code, args));
            }
            ServerMsg::QueuePosition {
                position,
                estimate_secs,
            } => self.queue = Some((position, estimate_secs)),
            ServerMsg::Dialogue { npc, text, options } => {
                self.dialogue = Some(OpenDialogue { npc, text, options });
                self.dialogue_changed = true;
            }
            ServerMsg::DialogueEnd { npc } => {
                if self.dialogue.as_ref().is_some_and(|d| d.npc == npc) {
                    self.dialogue = None;
                    self.dialogue_changed = true;
                }
            }
            ServerMsg::TextPack {
                locale,
                content_hash,
                part,
                parts,
                data,
            } => self.text_pack_part(locale, content_hash, part, parts, data),
            ServerMsg::TextPackChanged { content_hash } => self.content_hash = content_hash,
            ServerMsg::Definitions {
                items,
                classes,
                techniques,
                proficiencies,
                titles,
                sprites,
                icons,
                recipes,
                emotes,
                title_bonuses,
                emote_icons,
                zones,
                ..
            } => {
                self.zones = zones;
                self.title_bonuses = title_bonuses;
                self.emote_icons = emote_icons;
                self.recipes = recipes;
                self.emotes = emotes;
                self.sprites = sprites;
                self.icons = icons;
                self.items = items;
                self.classes = classes;
                self.techniques = techniques;
                self.proficiencies = proficiencies;
                self.titles = titles;
            }
            ServerMsg::Inventory {
                coins,
                slots,
                instances,
                equipped,
            } => {
                if let Some(old) = &self.inventory {
                    let before = holdings(&old.slots, &self.instances, &self.worn);
                    let after = holdings(&slots, &instances, &equipped);
                    self.gains.extend(gained(&before, &after));
                    let excess = self.gains.len().saturating_sub(32);
                    self.gains.drain(..excess);
                }
                self.inventory = Some(Inventory { coins, slots });
                self.instances = instances;
                self.worn = equipped;
                self.inventory_changed = true;
            }
            ServerMsg::System { text, window } => self.system.push((text, window)),
            ServerMsg::NpcMarkers { markers } => self.markers = markers.into_iter().collect(),
            ServerMsg::TechniqueUsed { events } => {
                self.supports.extend(events);
                let excess = self.supports.len().saturating_sub(64);
                self.supports.drain(..excess);
            }
            ServerMsg::Combat { events } => {
                // Keeps the latest blows only (floating numbers).
                self.combat.extend(events);
                let excess = self.combat.len().saturating_sub(256);
                self.combat.drain(..excess);
            }
            ServerMsg::Stats(st) => {
                self.stats = Some(*st);
                self.stats_at_ms = now_ms;
                self.used.clear();
                self.items_used.clear();
                self.stats_changed = true;
            }
            ServerMsg::QuestLog { quests } => {
                if self.quests_seen {
                    self.quest_news.extend(quest_news(&self.quests, &quests));
                }
                self.quests_seen = true;
                self.quests = quests;
                self.quests_changed = true;
            }
            ServerMsg::ChatMessage { channel, from, text } => {
                self.chat.push(ChatLine { channel, from, text });
                let excess = self.chat.len().saturating_sub(256);
                self.chat.drain(..excess);
            }
            ServerMsg::Appearances(a) => self.appearances = a,
            ServerMsg::Shops { npcs } => self.shop_npcs = npcs.into_iter().collect(),
            ServerMsg::Shop(view) => {
                self.shop = Some(*view);
                self.shop_changed = true;
            }
            ServerMsg::ShopEnd => {
                self.shop = None;
                self.shop_changed = true;
            }
            ServerMsg::Invite { from, kind } => {
                self.invites.retain(|i| *i != (from, kind));
                self.invites.push((from, kind));
                let excess = self.invites.len().saturating_sub(8);
                self.invites.drain(..excess);
            }
            ServerMsg::Party(view) => self.party = Some(*view),
            ServerMsg::PartyEnd => self.party = None,
            ServerMsg::Trade(view) => {
                // The asked side also gets the trade, closed, beside the
                // request: the request stays until the trade opens.
                if view.open {
                    self.invites
                        .retain(|i| *i != (view.with, protocol::InviteKind::Trade));
                }
                self.trade = Some(*view);
            }
            ServerMsg::TradeEnd => {
                // A request whose trade ended (cancelled, expired) is gone.
                if let Some(t) = self.trade.take() {
                    self.invites
                        .retain(|i| *i != (t.with, protocol::InviteKind::Trade));
                }
            }
            ServerMsg::Emote { entity, emote } => {
                self.emote_events.push((entity, emote));
                let excess = self.emote_events.len().saturating_sub(64);
                self.emote_events.drain(..excess);
            }
            // Login answers: handled by whoever logs in, before the game starts.
            ServerMsg::Session { .. } | ServerMsg::Characters { .. } | ServerMsg::Challenge { .. } => {}
        }
    }

    /// Joins the parts of a text pack; a part out of order drops the pack
    /// (the client asks again on the next content hash).
    fn text_pack_part(&mut self, locale: Locale, content_hash: u64, part: u16, parts: u16, data: Vec<u8>) {
        if part == 0 {
            self.pack_parts = Some((
                ReceivedTextPack {
                    locale,
                    content_hash,
                    data: Vec::new(),
                },
                0,
                parts,
            ));
        }
        let Some((mut pack, next, total)) = self.pack_parts.take() else {
            return;
        };
        if part != next || parts != total || pack.locale != locale || pack.content_hash != content_hash {
            return;
        }
        pack.data.extend_from_slice(&data);
        if next + 1 >= total {
            self.text_pack = Some(pack);
        } else {
            self.pack_parts = Some((pack, next + 1, total));
        }
    }

    pub fn stats(&self) -> Option<&StatsInfo> {
        self.stats.as_ref()
    }

    pub fn take_stats_changed(&mut self) -> bool {
        std::mem::take(&mut self.stats_changed)
    }

    /// System messages since the last call: `(text, opens a window)`.
    pub fn take_system(&mut self) -> Vec<(TextRef, bool)> {
        std::mem::take(&mut self.system)
    }

    /// Blows since the last call.
    pub fn take_combat(&mut self) -> Vec<CombatEvent> {
        std::mem::take(&mut self.combat)
    }

    /// Support techniques used since the last call.
    pub fn take_supports(&mut self) -> Vec<protocol::TechniqueUse> {
        std::mem::take(&mut self.supports)
    }

    pub fn class_info(&self, id: base::ClassId) -> Option<&NamedInfo> {
        self.classes.get(id.index())
    }

    /// Sprite name of an entity `appearance` or an equipment visual.
    pub fn sprite_name(&self, n: u16) -> Option<&str> {
        usize::from(n)
            .checked_sub(1)
            .and_then(|i| self.sprites.get(i))
            .map(String::as_str)
    }

    /// Icon name of an `icon` number; `None` for the generic icon.
    pub fn icon_name(&self, n: u16) -> Option<&str> {
        usize::from(n)
            .checked_sub(1)
            .and_then(|i| self.icons.get(i))
            .map(String::as_str)
    }

    /// Proficiency of the weapon in hand; `None` with empty hands.
    pub fn wielded(&self) -> Option<base::ProficiencyId> {
        let (_, item) = self
            .stats
            .as_ref()?
            .equipment
            .iter()
            .find(|(slot, _)| *slot == protocol::EquipSlot::Weapon)?;
        self.item(*item)?.weapon
    }

    /// The technique's weapon requirement holds with what is in hand now.
    pub fn weapon_fits(&self, id: base::TechniqueId) -> bool {
        self.technique_info(id)
            .is_some_and(|t| t.any_weapon || t.proficiency == self.wielded())
    }

    pub fn technique_info(&self, id: base::TechniqueId) -> Option<&TechniqueInfo> {
        self.techniques.get(id.index())
    }

    /// Milliseconds until a technique can be used again (its own cooldown
    /// or the global one, whichever ends later); 0 when ready.
    pub fn cooldown_left_ms(&self, id: base::TechniqueId, now_ms: f64) -> f64 {
        let Some(s) = &self.stats else { return 0.0 };
        let own = s.cooldowns.iter().find(|(t, _)| *t == id).map_or(0, |(_, n)| *n);
        let ticks = own.max(s.global_cooldown);
        let known = f64::from(ticks) * TICK_MS - (now_ms - self.stats_at_ms);
        let guess = self
            .used
            .iter()
            .map(|(t, at, ms)| {
                if *t == id {
                    ms - (now_ms - at)
                } else {
                    GLOBAL_GUESS_MS - (now_ms - at)
                }
            })
            .fold(0.0, f64::max);
        known.max(guess).max(0.0)
    }

    /// Seconds a companion of the list still rests; 0 when it can be called.
    pub fn companion_rest_secs(&self, index: usize, now_ms: f64) -> u32 {
        let Some(c) = self.stats.as_ref().and_then(|s| s.companions.get(index)) else {
            return 0;
        };
        let left = f64::from(c.rest_secs) - (now_ms - self.stats_at_ms) / 1000.0;
        left.max(0.0).ceil() as u32
    }

    /// Milliseconds until a consumable can be used again; 0 when ready.
    pub fn item_cooldown_left_ms(&self, item: ItemId, now_ms: f64) -> f64 {
        let known = self.stats.as_ref().map_or(0.0, |s| {
            let ticks = s
                .item_cooldowns
                .iter()
                .find(|(i, _)| *i == item)
                .map_or(0, |(_, n)| *n);
            f64::from(ticks) * TICK_MS - (now_ms - self.stats_at_ms)
        });
        let guess = self
            .items_used
            .iter()
            .filter(|(i, _, _)| *i == item)
            .map(|(_, at, ms)| ms - (now_ms - at))
            .fold(0.0, f64::max);
        known.max(guess).max(0.0)
    }

    /// A consumable was just used: its cooldown shows now, not when the
    /// server's answer comes back.
    pub fn note_item_used(&mut self, item: ItemId, now_ms: f64) {
        let Some(u) = self.item(item).and_then(|d| d.use_effect) else {
            return;
        };
        self.items_used
            .push((item, now_ms, f64::from(u.cooldown_ticks) * TICK_MS));
    }

    /// A technique was just sent: its cooldown starts on the bar now, not
    /// when the server's answer comes back.
    pub fn note_technique_used(&mut self, id: base::TechniqueId, now_ms: f64) {
        let Some(info) = self.technique_info(id) else {
            return;
        };
        let ms = f64::from(info.cooldown_ticks) * TICK_MS;
        self.used.push((id, now_ms, ms));
    }

    /// The quest marker over an NPC, if any.
    pub fn marker(&self, id: EntityId) -> Option<QuestMarker> {
        self.markers.get(&id).copied()
    }

    pub fn proficiency_info(&self, id: base::ProficiencyId) -> Option<&NamedInfo> {
        self.proficiencies.get(id.index())
    }

    pub fn title_info(&self, id: base::TitleId) -> Option<&NamedInfo> {
        self.titles.get(id.index())
    }

    /// What a title gives (empty when unknown).
    pub fn title_bonuses(&self, id: base::TitleId) -> &[protocol::TitleBonusInfo] {
        self.title_bonuses.get(id.index()).map_or(&[], Vec::as_slice)
    }

    /// Emotes of the content: `(id, name)`, in id order (phase 9).
    pub fn emotes(&self) -> impl Iterator<Item = (base::EmoteId, base::TextId)> + '_ {
        self.emotes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| Some((base::EmoteId::new(u32::try_from(i).ok()?), *n)))
    }

    /// The zone at `pos` and how many tiles it lies from the zone's nearest
    /// edge (0 on the edge row), for zone music.
    pub fn zone_at(&self, pos: base::TilePos) -> Option<(&str, i32)> {
        self.zones.iter().find_map(|z| {
            let inside = (z.min.x..=z.max.x).contains(&pos.x) && (z.min.y..=z.max.y).contains(&pos.y);
            let edge = (pos.x - z.min.x)
                .min(z.max.x - pos.x)
                .min(pos.y - z.min.y)
                .min(z.max.y - pos.y);
            inside.then_some((z.name.as_str(), edge))
        })
    }

    /// The name of an emote.
    pub fn emote_name(&self, id: base::EmoteId) -> Option<base::TextId> {
        self.emotes.get(id.index()).copied()
    }

    /// The picture of an emote: an icon name, `None` when it has none.
    pub fn emote_icon(&self, id: base::EmoteId) -> Option<&str> {
        let n = *self.emote_icons.get(id.index())?;
        self.icon_name(n)
    }

    /// Items gained since the last call: `(item, quantity)`.
    pub fn take_gains(&mut self) -> Vec<(ItemId, u32)> {
        std::mem::take(&mut self.gains)
    }

    /// Gestures that arrived since the last call.
    pub fn take_emotes(&mut self) -> Vec<(EntityId, base::EmoteId)> {
        std::mem::take(&mut self.emote_events)
    }

    /// The player's group (phase 9).
    pub fn party(&self) -> Option<&protocol::PartyView> {
        self.party.as_ref()
    }

    /// The player's trade (phase 9).
    pub fn trade(&self) -> Option<&protocol::TradeView> {
        self.trade.as_ref()
    }

    /// The newest invite or trade request not answered yet.
    pub fn invite(&self) -> Option<(EntityId, protocol::InviteKind)> {
        self.invites.last().copied()
    }

    /// `from` asked for this and nobody answered yet.
    pub fn has_invite(&self, from: EntityId, kind: protocol::InviteKind) -> bool {
        self.invites.contains(&(from, kind))
    }

    /// Forgets the newest invite (the player answered it).
    pub fn answer_invite(&mut self) {
        self.invites.pop();
    }

    /// Does this NPC keep a shop (phase 7)?
    pub fn keeps_shop(&self, id: EntityId) -> bool {
        self.shop_npcs.contains(&id)
    }

    /// The open shop (phase 7).
    pub fn shop(&self) -> Option<&protocol::ShopView> {
        self.shop.as_ref()
    }

    pub fn take_shop_changed(&mut self) -> bool {
        std::mem::take(&mut self.shop_changed)
    }

    /// Recipes of the content (phase 8); `RecipeId` is the position.
    pub fn recipe(&self, id: base::RecipeId) -> Option<&protocol::RecipeInfo> {
        self.recipes.get(id.index())
    }

    /// The worn pieces, `slot` 0 weapon and 1 armor (protocol 14).
    pub fn worn(&self) -> &[protocol::InstanceInfo] {
        &self.worn
    }

    pub fn instances(&self) -> &[protocol::InstanceInfo] {
        &self.instances
    }

    /// `Definitions` of an item.
    pub fn item(&self, id: ItemId) -> Option<&ItemInfo> {
        self.items.get(id.index())
    }

    pub fn inventory(&self) -> Option<&Inventory> {
        self.inventory.as_ref()
    }

    pub fn quests(&self) -> &[QuestEntry] {
        &self.quests
    }

    /// `true` once after a new `Inventory`.
    pub fn take_inventory_changed(&mut self) -> bool {
        std::mem::take(&mut self.inventory_changed)
    }

    /// `true` once after a new `QuestLog`.
    pub fn take_quests_changed(&mut self) -> bool {
        std::mem::take(&mut self.quests_changed)
    }

    /// Quests started, moved forward or completed since the last call.
    pub fn take_quest_news(&mut self) -> Vec<QuestNews> {
        std::mem::take(&mut self.quest_news)
    }

    pub fn me(&self) -> Option<&VisibleEntity> {
        self.entities.get(&self.me?)
    }

    /// Phase 10: how far into the day the world is, 0 (midnight) to 1;
    /// `None` before the server said. Counts on from the last `WorldTime`.
    pub fn day_fraction(&self, now_ms: f64) -> Option<f64> {
        let (secs, day, at) = self.world_time?;
        let now = f64::from(secs) + (now_ms - at).max(0.0) / 1000.0;
        Some((now / f64::from(day)).fract())
    }

    /// The weather where the player stands now (kind 0 rain, 1 snow,
    /// 2 fog), from the zone's chances and the world hour: every client
    /// of the zone shows the same.
    pub fn weather(&self, now_ms: f64) -> Option<u8> {
        let (secs, day_secs, at) = self.world_time?;
        let me = self.me()?;
        let zone = self
            .zones
            .iter()
            .find(|z| (z.min.x..=z.max.x).contains(&me.pos.x) && (z.min.y..=z.max.y).contains(&me.pos.y))?;
        let elapsed = u64::from(secs).saturating_add(((now_ms - at).max(0.0) / 1000.0) as u64);
        let day = self.world_day.saturating_add(elapsed / u64::from(day_secs));
        let of_day = u32::try_from(elapsed % u64::from(day_secs)).unwrap_or(0);
        let hour = base::world_hour(day, of_day, day_secs);
        let mut roll = base::hour_roll(&zone.name, hour);
        for (kind, permille) in &zone.weather {
            if roll < *permille {
                return Some(*kind);
            }
            roll -= permille;
        }
        None
    }

    pub fn my_id(&self) -> Option<EntityId> {
        self.me
    }

    pub fn entities(&self) -> impl Iterator<Item = &VisibleEntity> {
        self.entities.values()
    }

    pub fn entity(&self, id: EntityId) -> Option<&VisibleEntity> {
        self.entities.get(&id)
    }

    /// Tile word (`floor | TILE_BLOCKS`) if its chunk arrived.
    pub fn tile(&self, p: TilePos) -> Option<u16> {
        let (cx, cy) = protocol::chunk_of(p);
        let c = self.chunks.get(&(cx, cy))?;
        c.get(p.x.rem_euclid(CHUNK_SIZE), p.y.rem_euclid(CHUNK_SIZE))
    }

    /// The player at `from` sees tile `p`: within the view radius
    /// and in line of sight, the server's rule for who sees whom. Unknown
    /// before `Welcome` (radius 0): everything counts as seen.
    pub fn sees(&self, from: TilePos, p: TilePos) -> bool {
        if self.view_radius == 0 {
            return true;
        }
        let d = from.distance(p);
        d <= 1 || (d <= u32::from(self.view_radius) && protocol::sight_line(|t| self.tile(t), from, p))
    }

    pub fn view_radius(&self) -> u16 {
        self.view_radius
    }

    pub fn blocks(&self, p: TilePos) -> bool {
        self.tile(p).is_none_or(|w| w & TILE_BLOCKS != 0)
    }

    /// Drops chunks farther than `keep` chunks from `center`.
    pub fn forget_far_chunks(&mut self, center: TilePos, keep: i32) {
        self.chunks.retain(|c, _| protocol::chunk_kept(center, *c, keep));
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn content_hash(&self) -> u64 {
        self.content_hash
    }

    pub fn server_tick(&self) -> u32 {
        self.server_tick
    }

    /// `(position, estimate_secs)` while waiting in the entry queue.
    pub fn queue(&self) -> Option<(u32, u32)> {
        self.queue
    }

    pub fn dialogue(&self) -> Option<&OpenDialogue> {
        self.dialogue.as_ref()
    }

    /// `true` once after the dialogue opened, changed node or closed.
    pub fn take_dialogue_changed(&mut self) -> bool {
        std::mem::take(&mut self.dialogue_changed)
    }

    pub fn take_text_pack(&mut self) -> Option<ReceivedTextPack> {
        self.text_pack.take()
    }

    /// Chat lines since the last call.
    pub fn take_chat(&mut self) -> Vec<ChatLine> {
        std::mem::take(&mut self.chat)
    }

    /// Creation lists of the content (empty until a `Session`).
    pub fn appearances(&self) -> &AppearanceInfo {
        &self.appearances
    }

    /// Last error and its arguments (seconds, reason...).
    pub fn take_error(&mut self) -> Option<(ErrorCode, Vec<Arg>)> {
        self.last_error.take()
    }

    pub fn take_pongs(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pongs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base::CharacterName;
    use protocol::{CHUNK_TILES, Diff};

    #[test]
    fn zone_at_finds_the_zone_and_the_distance_to_its_edge() {
        let s = WorldState {
            zones: vec![protocol::ZoneInfo {
                name: "aurelia".into(),
                min: base::TilePos::new(0, 0),
                max: base::TilePos::new(9, 19),
                weather: Vec::new(),
            }],
            ..WorldState::default()
        };
        assert_eq!(s.zone_at(base::TilePos::new(0, 5)), Some(("aurelia", 0)));
        assert_eq!(s.zone_at(base::TilePos::new(4, 10)), Some(("aurelia", 4)));
        assert_eq!(s.zone_at(base::TilePos::new(9, 10)), Some(("aurelia", 0)));
        assert_eq!(s.zone_at(base::TilePos::new(10, 10)), None);
    }

    pub(super) fn ent(id: u32, x: i32, y: i32) -> Entity {
        Entity {
            id: EntityId::new(id),
            kind: EntityKind::Player,
            pos: TilePos::new(x, y),
            direction: Direction::S,
            appearance: 0,
            name: EntityName::Player(CharacterName::try_from("abc").unwrap()),
            health: 100,
            equipment: (0, 0, 0),
            light: 0,
            level: 1,
            look: Look::default(),
        }
    }

    #[test]
    fn queue_position_until_welcome() {
        let mut w = WorldState::new();
        w.apply(
            ServerMsg::QueuePosition {
                position: 3,
                estimate_secs: 40,
            },
            0.0,
        );
        assert_eq!(w.queue(), Some((3, 40)));
        w.apply(
            ServerMsg::Welcome {
                your_id: EntityId::new(1),
                pos: TilePos::new(0, 0),
                tick: 1,
                content_hash: 0,
                view_radius: 20,
            },
            0.0,
        );
        assert_eq!(w.queue(), None);
    }

    #[test]
    fn welcome_snapshot_diff() {
        let mut w = WorldState::new();
        w.apply(
            ServerMsg::Welcome {
                your_id: EntityId::new(1),
                pos: TilePos::new(0, 0),
                tick: 5,
                content_hash: 9,
                view_radius: 20,
            },
            0.0,
        );
        w.apply(
            ServerMsg::Snapshot {
                entities: vec![ent(1, 0, 0), ent(2, 3, 3)],
            },
            0.0,
        );
        assert_eq!(w.entities().count(), 2);
        assert_eq!(w.me().unwrap().pos, TilePos::new(0, 0));
        w.apply(
            ServerMsg::Diff(Diff {
                tick: 6,
                entered: vec![ent(3, 1, 1)],
                left: vec![EntityId::new(2)],
                moved: vec![(EntityId::new(1), TilePos::new(1, 0), Direction::E)],
                updated: vec![],
            }),
            1000.0,
        );
        let ids: Vec<u32> = w.entities().map(|e| e.id.get()).collect();
        assert_eq!(ids, [1, 3]);
        assert_eq!(w.me().unwrap().pos, TilePos::new(1, 0));
        assert_eq!(w.server_tick(), 6);
    }

    /// The asked side gets the trade, closed, beside the request: the
    /// request must stay until the trade opens or ends.
    #[test]
    fn trade_request_survives_the_closed_trade() {
        let mut w = WorldState::new();
        let from = EntityId::new(7);
        let view = |open| {
            ServerMsg::Trade(Box::new(protocol::TradeView {
                with: from,
                open,
                mine: Vec::new(),
                mine_coins: 0,
                theirs: Vec::new(),
                theirs_pieces: Vec::new(),
                theirs_coins: 0,
                mine_confirmed: false,
                theirs_confirmed: false,
            }))
        };
        let invite = ServerMsg::Invite {
            from,
            kind: protocol::InviteKind::Trade,
        };
        w.apply(invite.clone(), 0.0);
        w.apply(view(false), 0.0);
        assert!(w.has_invite(from, protocol::InviteKind::Trade));
        w.apply(view(true), 0.0);
        assert!(!w.has_invite(from, protocol::InviteKind::Trade));
        // Declined or expired: the trade and its request go together.
        w.apply(invite, 0.0);
        w.apply(view(false), 0.0);
        w.apply(ServerMsg::TradeEnd, 0.0);
        assert!(w.trade().is_none() && w.invite().is_none());
    }

    #[test]
    fn interpolation_without_jumps() {
        let mut e = VisibleEntity::new(ent(1, 0, 0), 0.0);
        e.move_to(TilePos::new(1, 0), Direction::E, 1000.0);
        assert_eq!(e.visual_pos(1000.0), (0.0, 0.0));
        assert_eq!(e.visual_pos(1100.0), (0.5, 0.0));
        assert_eq!(e.visual_pos(1300.0), (1.0, 0.0));
        // Next step arrives halfway: continues from the drawn position.
        e.move_to(TilePos::new(2, 0), Direction::E, 1300.0);
        e.move_to(TilePos::new(3, 0), Direction::E, 1400.0);
        assert_eq!(e.visual_pos(1400.0), (1.5, 0.0));
        // Two tiles behind would sprint at 2x in a fixed time; the time follows
        // the distance instead, with at most a little catching up (1.125x here).
        assert_eq!(e.visual_pos(1600.0), (2.625, 0.0));
        let (x, _) = e.visual_pos(1400.0 + 200.0 * 1.5 / 1.125);
        assert!((x - 3.0).abs() < 1e-9, "{x}");
        // A step along the screen's horizontal takes 7 ticks, not 4.
        e.move_to(TilePos::new(4, -1), Direction::NE, 1800.0);
        assert_eq!(e.visual_pos(1800.0 + 175.0), (3.5, -0.5));
        // Teleport-like jump: no slide across the map.
        e.move_to(TilePos::new(50, 0), Direction::E, 2000.0);
        assert_eq!(e.visual_pos(2000.0), (50.0, 0.0));
    }

    #[test]
    fn a_respawn_far_away_does_not_walk_on_the_spot() {
        // Dying far from the respawn point: the jump must not leave the walk
        // frames playing while the character stands on the new tile.
        let mut e = VisibleEntity::new(ent(1, 0, 0), 0.0);
        e.move_to(TilePos::new(30, 0), Direction::E, 1000.0);
        assert_eq!(e.visual_pos(1000.0), (30.0, 0.0));
        assert!(!e.moving(1000.0));
        assert!(!e.moving(1100.0));
        assert_eq!(e.visual_pos(1100.0), (30.0, 0.0));
    }

    #[test]
    fn tiles_from_chunks() {
        let mut w = WorldState::new();
        let mut words = vec![1u16; CHUNK_TILES];
        words[CHUNK_TILES - 1] = 2 | TILE_BLOCKS; // local (31, 31)
        w.apply(
            ServerMsg::Chunk {
                cx: -1,
                cy: 0,
                tiles: ChunkTiles::try_from(words).unwrap(),
            },
            0.0,
        );
        assert_eq!(w.tile(TilePos::new(-32, 0)), Some(1));
        assert!(w.blocks(TilePos::new(-1, 31)));
        assert!(w.blocks(TilePos::new(0, 0))); // unknown chunk
        w.forget_far_chunks(TilePos::new(1000, 0), 2);
        assert_eq!(w.chunk_count(), 0);
    }
}

#[cfg(test)]
mod dialogue_tests {
    use super::*;
    use base::TextId;

    #[test]
    fn dialogue_opens_changes_and_closes_once() {
        let mut w = WorldState::new();
        let npc = EntityId::new(7);
        w.apply(
            ServerMsg::Dialogue {
                npc,
                text: TextRef::new(TextId::new(1)),
                options: vec![TextRef::new(TextId::new(2))],
            },
            0.0,
        );
        assert!(w.take_dialogue_changed());
        assert!(!w.take_dialogue_changed());
        assert_eq!(w.dialogue().map(|d| d.options.len()), Some(1));
        // The end of another NPC's dialogue does not close this one.
        w.apply(
            ServerMsg::DialogueEnd {
                npc: EntityId::new(8),
            },
            0.0,
        );
        assert!(w.dialogue().is_some());
        w.apply(ServerMsg::DialogueEnd { npc }, 0.0);
        assert!(w.dialogue().is_none());
        assert!(w.take_dialogue_changed());
    }

    #[test]
    fn text_pack_is_taken_once() {
        let mut w = WorldState::new();
        w.apply(
            ServerMsg::TextPack {
                locale: Locale::from_static("pt-BR"),
                content_hash: 9,
                part: 0,
                parts: 1,
                data: vec![1, 2, 3],
            },
            0.0,
        );
        assert_eq!(w.take_text_pack().map(|p| p.content_hash), Some(9));
        assert!(w.take_text_pack().is_none());
    }

    fn part(part: u16, data: Vec<u8>) -> ServerMsg {
        ServerMsg::TextPack {
            locale: Locale::from_static("de"),
            content_hash: 4,
            part,
            parts: 3,
            data,
        }
    }

    #[test]
    fn text_pack_parts_join_in_order() {
        let mut w = WorldState::new();
        w.apply(part(0, vec![1]), 0.0);
        w.apply(part(1, vec![2]), 0.0);
        assert!(w.take_text_pack().is_none());
        w.apply(part(2, vec![3]), 0.0);
        assert_eq!(w.take_text_pack().map(|p| p.data), Some(vec![1, 2, 3]));
        // A missing part drops the pack.
        w.apply(part(0, vec![1]), 0.0);
        w.apply(part(2, vec![3]), 0.0);
        assert!(w.take_text_pack().is_none());
    }

    #[test]
    fn gains_count_new_stacks_and_new_pieces_only() {
        let slot = |item: u32, qty: u32| {
            Some(ItemSlot {
                item: ItemId::new(item),
                qty,
            })
        };
        let piece = |slot: u16, id: u64| protocol::InstanceInfo {
            slot,
            id,
            hidden: None,
            durability: (0, 0),
            crafter: None,
            rarity: base::Rarity::Common,
            power_permille: 0,
            mastery: 0,
            power: None,
        };
        let inv = |slots, instances, equipped| ServerMsg::Inventory {
            coins: 0,
            slots,
            instances,
            equipped,
        };
        let mut w = WorldState::new();
        // The first inventory is what the player had: no gain.
        w.apply(inv(vec![slot(0, 2), slot(5, 1)], vec![piece(1, 77)], vec![]), 0.0);
        assert!(w.take_gains().is_empty());
        // Two more ore and a new sword.
        w.apply(
            inv(
                vec![slot(0, 4), slot(5, 1), slot(5, 1)],
                vec![piece(1, 77), piece(2, 78)],
                vec![],
            ),
            0.0,
        );
        assert_eq!(w.take_gains(), vec![(ItemId::new(0), 2), (ItemId::new(5), 1)]);
        // Equipping the old sword is no gain.
        w.apply(
            inv(
                vec![slot(0, 4), None, slot(5, 1)],
                vec![piece(2, 78)],
                vec![piece(0, 77)],
            ),
            0.0,
        );
        assert!(w.take_gains().is_empty());
    }

    #[test]
    fn inventory_and_quests_flag_changes() {
        let mut w = WorldState::new();
        w.apply(
            ServerMsg::Inventory {
                instances: vec![],
                equipped: vec![],
                coins: 5,
                slots: vec![
                    Some(ItemSlot {
                        item: ItemId::new(0),
                        qty: 2,
                    }),
                    None,
                ],
            },
            0.0,
        );
        assert!(w.take_inventory_changed());
        assert!(!w.take_inventory_changed());
        assert_eq!(w.inventory().map(|i| i.coins), Some(5));
        w.apply(ServerMsg::QuestLog { quests: vec![] }, 0.0);
        assert!(w.take_quests_changed());
        assert!(w.take_quest_news().is_empty());
        w.apply(ServerMsg::TextPackChanged { content_hash: 77 }, 0.0);
        assert_eq!(w.content_hash(), 77);
    }

    #[test]
    fn chat_appearances_and_error_args() {
        let mut w = WorldState::new();
        let name = CharacterName::try_from("Ana").unwrap();
        w.apply(
            ServerMsg::ChatMessage {
                channel: ChatChannel::Whisper(name.clone()),
                from: name.clone(),
                text: ChatText::try_from("oi").unwrap(),
            },
            0.0,
        );
        w.apply(
            ServerMsg::Appearances(AppearanceInfo {
                lists: [vec![], vec![], vec![], vec![]],
            }),
            0.0,
        );
        w.apply(
            ServerMsg::Error {
                code: ErrorCode::Muted,
                args: vec![Arg::Int(30)],
            },
            0.0,
        );
        let chat = w.take_chat();
        assert_eq!((chat.len(), chat[0].from.as_str()), (1, "Ana"));
        assert!(w.take_chat().is_empty());
        assert_eq!(w.take_error(), Some((ErrorCode::Muted, vec![Arg::Int(30)])));
        let mut e = super::tests::ent(5, 0, 0);
        e.look.hair_color = 3;
        w.apply(ServerMsg::Snapshot { entities: vec![e] }, 0.0);
        assert_eq!(w.entity(EntityId::new(5)).unwrap().look.hair_color, 3);
    }

    fn quest(name: u32, state: QuestState, objectives: Vec<ObjectiveView>) -> QuestEntry {
        QuestEntry {
            name: base::TextId::new(name),
            description: base::TextId::new(name + 1),
            state,
            objectives,
            goal: None,
        }
    }

    fn herbs(have: u32) -> ObjectiveView {
        ObjectiveView::Item {
            item: ItemId::new(3),
            have,
            need: 3,
        }
    }

    #[test]
    fn quest_news_only_tells_changes() {
        use QuestState::{Active, Completed};
        let mut w = WorldState::new();
        // The first log is the baseline: nothing to tell.
        w.apply(
            ServerMsg::QuestLog {
                quests: vec![quest(1, Completed, vec![herbs(0)])],
            },
            0.0,
        );
        assert!(w.take_quest_news().is_empty());
        // The same log again (the backpack changed): still nothing.
        w.apply(
            ServerMsg::QuestLog {
                quests: vec![quest(1, Completed, vec![herbs(0)])],
            },
            0.0,
        );
        assert!(w.take_quest_news().is_empty());
        let log = |q: Vec<QuestEntry>| ServerMsg::QuestLog { quests: q };
        w.apply(
            log(vec![
                quest(1, Completed, vec![]),
                quest(5, Active, vec![herbs(0)]),
            ]),
            0.0,
        );
        assert_eq!(
            w.take_quest_news(),
            vec![QuestNews::Started(base::TextId::new(5))]
        );
        w.apply(
            log(vec![
                quest(1, Completed, vec![]),
                quest(5, Active, vec![herbs(1)]),
            ]),
            0.0,
        );
        assert_eq!(
            w.take_quest_news(),
            vec![QuestNews::Progress {
                quest: base::TextId::new(5),
                objective: herbs(1)
            }]
        );
        // Past the amount needed, or handing items over: no line.
        w.apply(
            log(vec![
                quest(1, Completed, vec![]),
                quest(5, Active, vec![herbs(3)]),
            ]),
            0.0,
        );
        assert_eq!(w.take_quest_news().len(), 1);
        w.apply(
            log(vec![
                quest(1, Completed, vec![]),
                quest(5, Active, vec![herbs(4)]),
            ]),
            0.0,
        );
        assert!(w.take_quest_news().is_empty());
        w.apply(
            log(vec![
                quest(1, Completed, vec![]),
                quest(5, Completed, vec![herbs(0)]),
            ]),
            0.0,
        );
        assert_eq!(
            w.take_quest_news(),
            vec![QuestNews::Completed(base::TextId::new(5))]
        );
    }
}
