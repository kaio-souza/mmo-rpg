mod commands;
mod entities;
mod reload;
mod snapshot;
mod spawn;

use crate::Content;
use crate::actions::place_bag;
use crate::craft::NodeComp;
use crate::fight::{self, Fighter, MonsterComp};
use crate::gear::Gear;
use crate::growth::Growth;
use crate::player::PlayerData;
use crate::replay::{ReplayLog, TickRecord, ZoneState};
use crate::scratch::Scratch;
use crate::shop::{ShopComp, ShopOpen};
use crate::social::Social;
use crate::types::{CharacterSheet, Command, DialogueOut, EntityState, Refusal, TickInput, TickOutput};
use crate::{interest, movement};
use base::{
    DialogueId, EntityId, Fnv64, IdBlock, NpcId, PlayerId, RegionId, Rng64, TextId, Tick, TilePos, ZoneId,
};
use defs::{MonsterSpawn, RegionDef};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;
use world::{MapError, SpatialGrid, Tile, TileMap};

/// Ticks per walked tile; a step along the screen's horizontal takes
/// `base::WIDE_STEP_TICKS` instead (`Direction::step_ticks`).
pub const STEP_TICKS: u64 = base::STEP_TICKS;
/// Farthest `Move` destination, in tiles (same as `protocol::MAX_MOVE_DISTANCE`).
pub const MAX_MOVE_DISTANCE: u32 = 32;
/// A* gives up after this many expanded nodes.
pub const PATH_MAX_NODES: u32 = 4096;
/// A walker waits this long for an occupied tile before giving up (2 s).
pub const BLOCKED_GIVE_UP_TICKS: u32 = 40;
/// How close a player must be to talk to an NPC, in tiles.
pub const TALK_RADIUS: u32 = 2;
/// How far from the spawn to look for a free tile when it is crowded.
const SPAWN_SEARCH_RADIUS: i32 = 8;

// Components.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Body {
    pub id: EntityId,
    pub pos: TilePos,
    pub facing: base::Direction,
    /// Last tick this entity changed tile (interest sends it as `moved`).
    pub moved_tick: u64,
    /// Last tick its HP or look changed (interest sends it as `updated`).
    pub status_tick: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Walk {
    pub steps: VecDeque<TilePos>,
    /// Earliest tick of the next step. Re-sending `Move` does not reset it.
    pub ready: u64,
    /// Ticks spent waiting for an occupied tile to clear.
    pub blocked: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PlayerComp {
    pub player: PlayerId,
    /// Name and look; coins, backpack, quests and affinity live in `data`
    /// while in the zone and are written back on the way out.
    pub sheet: crate::CharacterSheet,
    pub data: PlayerData,
    /// Entities this player knows about, sorted.
    pub known: Vec<EntityId>,
    /// The backpack changed: send `Inventory` at the end of the tick.
    pub dirty_bag: bool,
    /// Quests or anything they count changed: send `QuestLog`.
    pub dirty_quests: bool,
    /// Level, attributes, class, proficiencies, techniques, titles, counters.
    pub growth: Growth,
    /// Level, HP, points or equipment changed: send `Stats`.
    pub dirty_stats: bool,
    /// Set by death, saved with the sheet.
    pub locked_until: u64,
    /// Phase 5: tick at which each used consumable may be used again,
    /// sorted (not saved: cooldowns are short).
    pub item_ready: Vec<(base::ItemId, u64)>,
    /// Phase 7: the list of NPCs with a shop still has to be sent (on
    /// entering and after a reload).
    pub dirty_shops: bool,
    /// Phase 9: first tick the player may emote again.
    pub emote_ready: u64,
}

/// A content NPC placed in the zone by `npcs.ron`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NpcComp {
    pub npc: NpcId,
    /// `npc.<id>.name`, translated by the client.
    pub name: TextId,
    pub dialogue: Option<DialogueId>,
}

/// The dialogue a player currently has open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DialogueComp {
    pub npc: EntityId,
    pub dialogue: DialogueId,
    pub node: u16,
    /// Positions in the node of the options shown, in order: `ChooseOption`
    /// picks among these.
    pub shown: Vec<u16>,
}

#[derive(Debug)]
pub enum ZoneError {
    UnknownZone(ZoneId),
    Map(MapError),
    /// New content without an item, quest or NPC the running one has:
    /// removing needs a migration and a restart.
    Removed(String),
}

impl fmt::Display for ZoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownZone(id) => write!(f, "zone {} not in the catalog", id.get()),
            Self::Map(e) => write!(f, "zone map: {e}"),
            Self::Removed(what) => write!(f, "new content removes {what}: restart to apply"),
        }
    }
}

impl std::error::Error for ZoneError {}

/// One zone. `Send`: the binary runs each zone as a task in its pool.
pub struct Zone {
    pub(crate) world: hecs::World,
    pub(crate) map: TileMap,
    pub(crate) grid: SpatialGrid,
    pub(crate) view_radius: u16,
    /// Definitions; replaced only by `reload`.
    pub(crate) content: Arc<Content>,
    zone_id: ZoneId,
    spawn: TilePos,
    spawn_radius: u16,
    pub(crate) seed: u64,
    pub(crate) tick: Tick,
    pub(crate) next_id: u32,
    pub(crate) by_id: BTreeMap<EntityId, hecs::Entity>,
    pub(crate) by_player: BTreeMap<PlayerId, hecs::Entity>,
    /// `Command::Taken` for players not in the zone yet: applied when they
    /// enter or arrive, before their titles are checked.
    pub(crate) early_taken: BTreeMap<PlayerId, Vec<String>>,
    /// Unique items that exist somewhere on the server (`Command::TakenItems`
    /// plus those made here since): never made again.
    pub(crate) taken_items: std::collections::BTreeSet<base::ItemId>,
    /// Teleports asked this tick (a dialogue), done after movement.
    pub(crate) teleports: Vec<(PlayerId, TilePos)>,
    replay: ReplayLog,
    /// The zone's random numbers (combat, loot, spawns): saved in snapshots.
    pub(crate) rng: Rng64,
    /// Item ids reserved in the database (`Command::ItemIds`).
    pub(crate) item_ids: Option<IdBlock>,
    /// `(tick, entry of monsters.ron)` waiting to spawn.
    pub(crate) respawns: Vec<(u64, u16)>,
    pub(crate) regions: Vec<RegionDef>,
    pub(crate) monster_spawns: Vec<MonsterSpawn>,
    /// Wall clock of the current tick (from `TickInput`).
    pub(crate) now_unix: u64,
    /// Buffers the tick reuses; never state ([`crate::scratch`]).
    pub(crate) scratch: Scratch,
    /// Phase 9: groups, invites and trades.
    pub(crate) social: Social,
    /// Phase 10: own tiles and neighbors (fixed by the content).
    pub(crate) seam: crate::seam::Seam,
    /// Phase 10: entities of the neighbors near the seam.
    pub(crate) ghosts: crate::seam::Ghosts,
    /// Rule 4: deposits waiting for the database.
    pub(crate) escrows: crate::escrow::Escrows,
}

impl Zone {
    pub fn new(content: Arc<Content>, zone: ZoneId, seed: u64) -> Result<Zone, ZoneError> {
        let catalog = &content.catalog;
        let def = catalog.zone(zone).ok_or(ZoneError::UnknownZone(zone))?;
        let map = crate::seam::world_map(catalog, zone)?;
        let grid = SpatialGrid::new(&map);
        let mut zone = Zone {
            world: hecs::World::new(),
            map,
            grid,
            view_radius: def.view_radius,
            content: content.clone(),
            zone_id: zone,
            spawn: def.spawn,
            spawn_radius: def.spawn_radius,
            seed,
            tick: Tick::ZERO,
            next_id: crate::seam::first_entity_id(zone),
            by_id: BTreeMap::new(),
            by_player: BTreeMap::new(),
            early_taken: BTreeMap::new(),
            teleports: Vec::new(),
            taken_items: std::collections::BTreeSet::new(),
            replay: ReplayLog::new(def.name.clone(), catalog.content_hash(), seed),
            rng: Rng64::new(seed),
            item_ids: None,
            respawns: Vec::new(),
            regions: def.regions.clone(),
            monster_spawns: def.monsters.clone(),
            now_unix: 0,
            scratch: Scratch::default(),
            social: Social::default(),
            seam: crate::seam::Seam::of(catalog, zone),
            ghosts: crate::seam::Ghosts::default(),
            escrows: crate::escrow::Escrows::default(),
        };
        zone.spawn_npcs(def);
        zone.spawn_nodes(def);
        zone.spawn_all_monsters();
        let state = zone.snapshot();
        zone.replay.start(state);
        Ok(zone)
    }

    pub fn tick_count(&self) -> Tick {
        self.tick
    }

    pub fn map(&self) -> &TileMap {
        &self.map
    }

    pub fn view_radius(&self) -> u16 {
        self.view_radius
    }

    pub fn player_count(&self) -> usize {
        self.by_player.len()
    }

    pub fn replay_log(&self) -> &ReplayLog {
        &self.replay
    }

    pub fn position_of(&self, player: PlayerId) -> Option<TilePos> {
        let e = *self.by_player.get(&player)?;
        self.world.get::<&Body>(e).ok().map(|b| b.pos)
    }

    /// Sheet and position of every player, sorted by `PlayerId`: one
    /// consistent picture of the zone for a batch save.
    pub fn players(&self) -> Vec<(PlayerId, CharacterSheet, TilePos)> {
        self.by_player
            .iter()
            .filter_map(|(player, &h)| {
                let pos = self.world.get::<&Body>(h).ok()?.pos;
                Some((*player, self.export_handle(h)?, pos))
            })
            .collect()
    }

    /// Every entity of the zone as a client would see it, sorted by id:
    /// what the replay viewer draws (read only, no effect on the state).
    pub fn entity_states(&self) -> Vec<EntityState> {
        self.by_id
            .keys()
            .filter_map(|id| self.entity_state(*id))
            .collect()
    }

    /// A repeatable quest whose cooldown ended since `before` is offered
    /// again: its player's markers and log are sent anew.
    fn cooldowns_ended(&mut self, before: u64) {
        let cat = &self.content.catalog;
        let now = self.now_unix;
        for (_, p) in self.world.query_mut::<&mut PlayerComp>() {
            if p.data.cooldown_ended(cat, before, now) {
                p.dirty_quests = true;
            }
        }
    }

    /// Runs one tick. `out` is cleared first and reused.
    pub fn tick(&mut self, input: &TickInput, out: &mut TickOutput) {
        self.tick = self.tick.next();
        out.clear();
        out.tick = self.tick;
        let before = std::mem::replace(&mut self.now_unix, input.now_unix);
        if before != 0 && before < input.now_unix {
            self.cooldowns_ended(before);
        }
        for cmd in input.commands {
            self.apply(cmd, out);
        }
        movement::run(self);
        crate::portal::reached(self);
        crate::portal::run(self, out);
        fight::ai(self, out);
        fight::attacks(self, out);
        crate::status::run(self, out);
        fight::regen(self);
        fight::burn(self, out);
        fight::fallen(self, out);
        fight::respawn(self);
        self.close_far_dialogues(out);
        self.close_far_shops(out);
        self.flush_player_views(out);
        self.flush_shops(out);
        self.flush_shop_npcs(out);
        self.run_social(out);
        self.drain_coins(out);
        self.expire_ghosts();
        interest::run(self, out);
        // After the views: the step that crossed reaches its player from
        // this zone, on time; the next zone goes on from there.
        self.handoffs(out);
        self.border(out);
        let record = TickRecord {
            tick: self.tick,
            now_unix: input.now_unix,
            load: input.load,
            commands: input.commands.to_vec(),
            state_hash: self.state_hash(),
        };
        if self.replay.push(record) {
            let state = self.snapshot();
            self.replay.set_checkpoint(state);
        }
        // The world and the two indexes have to agree
        // ([`crate::zone::entities`]); only debug builds pay for the check.
        debug_assert!(
            self.index_error().is_none(),
            "zone index: {}",
            self.index_error().unwrap_or_default()
        );
    }

    /// Sheet with the zone's data written back as text ids.
    fn export(&self, p: &PlayerComp, gear: &Gear) -> CharacterSheet {
        let cat = &self.content.catalog;
        let mut sheet = p.sheet.clone();
        p.data.export(cat, &mut sheet);
        sheet.progress = p.growth.export(cat);
        sheet.items = gear.export(cat);
        sheet.locked_until = p.locked_until;
        sheet
    }

    /// Exports one player in the zone (sheet, HP), without removing it.
    pub(crate) fn export_handle(&self, h: hecs::Entity) -> Option<CharacterSheet> {
        let p = self.world.get::<&PlayerComp>(h).ok()?;
        let gear = self.world.get::<&Gear>(h).ok()?;
        let mut sheet = self.export(&p, &gear);
        sheet.hp = self.world.get::<&Fighter>(h).map_or(0, |f| f.hp);
        Some(sheet)
    }

    /// `Inventory` and `QuestLog` for players whose data changed (with the
    /// quest markers of the zone's NPCs).
    fn flush_player_views(&mut self, out: &mut TickOutput) {
        let content = self.content.clone();
        let now = self.now_unix;
        // Quest markers read every NPC of the zone, and only a player whose
        // quests changed asks for them: on a tick where nothing changed the
        // list is not built at all.
        let mut npcs = std::mem::take(&mut self.scratch.npcs);
        npcs.clear();
        if self
            .world
            .query_mut::<&PlayerComp>()
            .into_iter()
            .any(|(_, p)| p.dirty_bag || p.dirty_quests)
        {
            npcs.extend(self.by_id.iter().filter_map(|(id, h)| {
                self.world
                    .get::<&NpcComp>(*h)
                    .ok()
                    .map(|n| (*id, n.npc, n.dialogue))
            }));
        }
        self.with_players(|z, players| {
            for &(player, handle) in players {
                let pos = z.world.get::<&Body>(handle).map(|b| b.pos).ok();
                let Ok(p) = z.world.query_one_mut::<&mut PlayerComp>(handle) else {
                    continue;
                };
                if p.dirty_bag {
                    p.dirty_bag = false;
                    // Item objectives of active quests count the backpack;
                    // completed quests do not change with it.
                    p.dirty_quests |= p.data.quests.iter().any(|q| !q.completed);
                    out.inventories.push((player, p.data.inventory_view()));
                }
                if p.dirty_quests {
                    p.dirty_quests = false;
                    out.quest_logs.push((player, p.data.quest_views(&content, pos)));
                    let marks = npcs
                        .iter()
                        .filter_map(|(id, npc, dialogue)| {
                            Some((*id, p.data.marker(&content.catalog, *npc, *dialogue, now)?))
                        })
                        .collect();
                    out.markers.push((player, marks));
                }
            }
            for &(player, handle) in players {
                if let Some(stats) = z.stats_view(handle) {
                    out.stats.push((player, stats));
                }
            }
        });
        npcs.clear();
        self.scratch.npcs = npcs;
        // Backpacks with items with identity carry their details.
        for (player, view) in &mut out.inventories {
            if let Some(&h) = self.by_player.get(player)
                && let Ok(g) = self.world.get::<&Gear>(h)
            {
                let cat = &self.content.catalog;
                view.instances = g.bag.iter().map(|(slot, i)| i.view(*slot, cat)).collect();
                view.instances.sort_by_key(|i| i.slot);
                // What is worn too: slot 0 weapon, 1 armor, 2 off hand.
                view.equipped = g.equipped.iter().map(|(s, i)| i.view(s.index(), cat)).collect();
            }
        }
    }

    /// Moves the coins created and destroyed this tick out of the players
    /// and into the output, for the economy telemetry (phase 7).
    fn drain_coins(&mut self, out: &mut TickOutput) {
        for (_, p) in self.world.query_mut::<&mut PlayerComp>() {
            out.coins.append(&mut p.data.coin_log);
        }
        out.coins.sort();
    }

    pub fn content(&self) -> &Arc<Content> {
        &self.content
    }

    /// Phase 10: the zones beyond this zone's seam.
    pub fn neighbors(&self) -> Vec<ZoneId> {
        self.seam.neighbors.iter().map(|(z, _)| *z).collect()
    }

    pub fn zone_id(&self) -> ZoneId {
        self.zone_id
    }
}

/// The tile map of a zone definition (also used by the server for the
/// chunks it sends).
pub fn zone_map(def: &defs::ZoneDef) -> Result<TileMap, MapError> {
    let tiles = def
        .tiles
        .iter()
        .map(|t| Tile {
            floor: t.floor,
            blocks_movement: t.blocks_movement,
            blocks_sight: t.blocks_sight,
            region: RegionId::new(0),
            switch: t.switch.unwrap_or(0),
        })
        .collect();
    TileMap::new(def.origin, def.width, def.height, tiles)
}
