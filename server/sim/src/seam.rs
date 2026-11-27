//! Phase 10: the seam between zones.
//!
//! A zone's map is its own tiles plus a band of `SEAM_BAND` tiles of each
//! neighbor, so a player walks up to and over the edge without a wall. The
//! entities of a neighbor near the seam are **ghosts**: a picture (position,
//! look, HP) that the neighbor sends every tick it changes, used by the
//! views; a ghost has no logic and cannot be fought from this side yet.
//!
//! A player who walks `HANDOFF_DEPTH` tiles past the edge leaves the zone
//! as an [`Arrival`] for the zone of that tile (the handoff); the binary
//! carries it there. Monsters and NPCs never leave their own tiles, and a
//! player only walks onto a neighbor's tiles while that neighbor is heard
//! from (its ghosts keep arriving).

use crate::fight::Fighter;
use crate::types::{CharacterSheet, EntityState, TickOutput};
use crate::zone::{Body, PlayerComp, Walk};
use crate::{Zone, ZoneError};
use base::{EntityId, PlayerId, RegionId, TilePos, ZoneId};
use defs::{HANDOFF_DEPTH, Rect, SEAM_BAND, ValidatedCatalog};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use world::{Tile, TileMap};

/// Ghosts of a neighbor not heard from for this long expire (1 s), and
/// its tiles close to players until it is heard again.
pub const GHOST_TIMEOUT_TICKS: u64 = 20;
/// Entity ids are unique in the whole world: zone `z` numbers its own from
/// `z << ENTITY_ID_BITS`, so a ghost or a player handed over keeps its id.
pub const ENTITY_ID_BITS: u32 = 22;

/// The first entity id a zone gives out.
pub(crate) fn first_entity_id(zone: ZoneId) -> u32 {
    zone.get()
        .checked_shl(ENTITY_ID_BITS)
        .unwrap_or(0)
        .saturating_add(1)
}

/// Where this zone ends and who is beyond each side.
#[derive(Clone, Debug, Default)]
pub(crate) struct Seam {
    pub own: Option<Rect>,
    pub neighbors: Vec<(ZoneId, Rect)>,
}

impl Seam {
    pub(crate) fn of(cat: &ValidatedCatalog, zone: ZoneId) -> Seam {
        let Some(def) = cat.zone(zone) else {
            return Seam::default();
        };
        let all: Vec<&defs::ZoneDef> = cat.zones().map(|(_, z)| z).collect();
        let neighbors = defs::neighbors(def, all.iter().copied())
            .into_iter()
            .filter_map(|n| Some((cat.zone_by_name(&n.name)?.0, n.rect())))
            .collect();
        Seam {
            own: Some(def.rect()),
            neighbors,
        }
    }

    /// The tile is the zone's own (no seam: everything is).
    pub(crate) fn owns(&self, p: TilePos) -> bool {
        self.own.is_none_or(|r| r.contains(p))
    }

    /// The neighbor that owns a tile.
    pub(crate) fn owner(&self, p: TilePos) -> Option<ZoneId> {
        self.neighbors
            .iter()
            .find(|(_, r)| r.contains(p))
            .map(|(z, _)| *z)
    }
}

/// A neighbor's entity as this zone sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ghost {
    pub from: ZoneId,
    pub state: EntityState,
    /// Tick it last changed tile / HP or look here (the views send it).
    pub moved_tick: u64,
    pub status_tick: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ghosts {
    pub by_id: BTreeMap<EntityId, Ghost>,
    /// Last tick each neighbor was heard from.
    pub heard: BTreeMap<ZoneId, u64>,
}

/// What changed near the seam since the last message to that neighbor:
/// entities new or changed (whole picture) and entities gone. An empty one
/// still says "alive".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhostDiff {
    pub upsert: Vec<EntityState>,
    pub gone: Vec<EntityId>,
}

impl GhostDiff {
    pub fn is_empty(&self) -> bool {
        self.upsert.is_empty() && self.gone.is_empty()
    }

    /// The difference between two pictures of the band, both sorted by id.
    pub fn between(old: &[EntityState], new: &[EntityState]) -> GhostDiff {
        let mut d = GhostDiff::default();
        let (mut i, mut j) = (0, 0);
        loop {
            match (old.get(i), new.get(j)) {
                (Some(a), Some(b)) if a.id == b.id => {
                    if a != b {
                        d.upsert.push(b.clone());
                    }
                    i = i.saturating_add(1);
                    j = j.saturating_add(1);
                }
                (Some(a), Some(b)) if a.id < b.id => {
                    d.gone.push(a.id);
                    i = i.saturating_add(1);
                }
                (Some(a), None) => {
                    d.gone.push(a.id);
                    i = i.saturating_add(1);
                }
                (_, Some(b)) => {
                    d.upsert.push(b.clone());
                    j = j.saturating_add(1);
                }
                (None, None) => break,
            }
        }
        d
    }
}

/// A player crossing the seam: everything the next zone needs to go on as
/// if nothing happened (same entity id, HP, vigor, MP and walk).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arrival {
    pub entity: EntityId,
    pub sheet: CharacterSheet,
    pub pos: TilePos,
    pub facing: base::Direction,
    /// End of the walk in progress, if any: the next zone plans it again.
    pub destination: Option<TilePos>,
    /// Ticks until the next step may start (the pace of the walk): the
    /// next zone waits them out instead of stepping on arrival.
    pub step_wait: u64,
    pub vigor: u32,
    pub mp: u32,
    /// A teleport: the tile it left from, where it is saved if the next
    /// zone is down (`pos` may be far from the zone it left).
    #[serde(default)]
    pub left_from: Option<TilePos>,
    /// A teleport: the leader of the player's group then (the player
    /// itself when alone). The gateway puts a party in one copy of an
    /// instance.
    #[serde(default)]
    pub party: Option<PlayerId>,
}

/// The tile map a zone runs on: its own tiles and, with neighbors, a band
/// of `SEAM_BAND` tiles around them taken from whatever zone owns each
/// tile (none: closed). Without neighbors, exactly the zone's map.
pub fn world_map(cat: &ValidatedCatalog, zone: ZoneId) -> Result<TileMap, ZoneError> {
    let def = cat.zone(zone).ok_or(ZoneError::UnknownZone(zone))?;
    let seam = Seam::of(cat, zone);
    if seam.neighbors.is_empty() {
        return crate::zone_map(def).map_err(ZoneError::Map);
    }
    let area = def.rect().grow(SEAM_BAND);
    let span = |lo: i32, hi: i32| hi.saturating_sub(lo).saturating_add(1);
    let (w, h) = (span(area.min.x, area.max.x), span(area.min.y, area.max.y));
    let mut tiles =
        Vec::with_capacity(usize::try_from(i64::from(w).saturating_mul(i64::from(h))).unwrap_or(0));
    for y in area.min.y..=area.max.y {
        for x in area.min.x..=area.max.x {
            let p = TilePos::new(x, y);
            let t = def.tile_at(p).or_else(|| {
                seam.neighbors
                    .iter()
                    .find(|(_, r)| r.contains(p))
                    .and_then(|(z, _)| cat.zone(*z)?.tile_at(p))
            });
            tiles.push(match t {
                Some(t) => Tile {
                    floor: t.floor,
                    blocks_movement: t.blocks_movement,
                    blocks_sight: t.blocks_sight,
                    region: RegionId::new(0),
                    switch: t.switch.unwrap_or(0),
                },
                None => Tile {
                    floor: 0,
                    blocks_movement: true,
                    blocks_sight: true,
                    region: RegionId::new(0),
                    switch: 0,
                },
            });
        }
    }
    TileMap::new(area.min, w, h, tiles).map_err(ZoneError::Map)
}

impl Zone {
    /// The neighbor is up: its ghosts arrived not long ago.
    pub(crate) fn neighbor_alive(&self, zone: ZoneId) -> bool {
        self.ghosts
            .heard
            .get(&zone)
            .is_some_and(|t| self.tick.0.saturating_sub(*t) <= GHOST_TIMEOUT_TICKS)
    }

    /// May an entity step on this tile? Own tiles always; a neighbor's only
    /// for players, and only while that neighbor is up.
    pub(crate) fn may_step(&self, p: TilePos, player: bool) -> bool {
        if self.seam.owns(p) {
            return true;
        }
        player && self.seam.owner(p).is_some_and(|z| self.neighbor_alive(z))
    }

    /// `Command::Ghosts`: a neighbor's picture of its side of the seam.
    pub(crate) fn apply_ghosts(&mut self, from: ZoneId, diff: &GhostDiff) {
        let now = self.tick.0;
        self.ghosts.heard.insert(from, now);
        for id in &diff.gone {
            if self.ghosts.by_id.get(id).is_some_and(|g| g.from == from) {
                self.ghosts.by_id.remove(id);
            }
        }
        for s in &diff.upsert {
            match self.ghosts.by_id.get_mut(&s.id) {
                Some(g) => {
                    if (g.state.pos, g.state.direction) != (s.pos, s.direction) {
                        g.moved_tick = now;
                    }
                    if (
                        g.state.health,
                        g.state.equipment,
                        g.state.light,
                        g.state.level,
                        g.state.appearance,
                        g.state.look,
                    ) != (s.health, s.equipment, s.light, s.level, s.appearance, s.look)
                    {
                        g.status_tick = now;
                    }
                    g.from = from;
                    g.state = s.clone();
                }
                None => {
                    self.ghosts.by_id.insert(
                        s.id,
                        Ghost {
                            from,
                            state: s.clone(),
                            moved_tick: now,
                            status_tick: now,
                        },
                    );
                }
            }
        }
    }

    /// Ghosts of neighbors gone quiet expire; so does a ghost that is now a
    /// real entity here (a player handed over).
    pub(crate) fn expire_ghosts(&mut self) {
        let now = self.tick.0;
        let heard = &self.ghosts.heard;
        let by_id = &self.by_id;
        self.ghosts.by_id.retain(|id, g| {
            heard
                .get(&g.from)
                .is_some_and(|t| now.saturating_sub(*t) <= GHOST_TIMEOUT_TICKS)
                && !by_id.contains_key(id)
        });
    }

    /// Players `HANDOFF_DEPTH` tiles past the edge leave for the zone of
    /// their tile. Runs after the views, so the crossing step is sent from
    /// here, and before the ghosts.
    pub(crate) fn handoffs(&mut self, out: &mut TickOutput) {
        let Some(own) = self.seam.own else { return };
        if self.seam.neighbors.is_empty() {
            return;
        }
        let leaving: Vec<(PlayerId, ZoneId)> = self
            .by_player
            .iter()
            .filter_map(|(p, h)| {
                let pos = self.world.get::<&Body>(*h).ok()?.pos;
                if own.distance_to(pos) < HANDOFF_DEPTH {
                    return None;
                }
                Some((*p, self.seam.owner(pos)?))
            })
            .collect();
        for (player, to) in leaving {
            let Some(&h) = self.by_player.get(&player) else {
                continue;
            };
            let (vigor, mp) = self.world.get::<&Fighter>(h).map_or((0, 0), |f| (f.vigor, f.mp));
            let facing = self
                .world
                .get::<&Body>(h)
                .map_or(base::Direction::S, |b| b.facing);
            let (destination, step_wait) = self.world.get::<&Walk>(h).map_or((None, 0), |w| {
                (w.steps.back().copied(), w.ready.saturating_sub(self.tick.0))
            });
            let Some((sheet, pos, entity)) = self.take_player(player) else {
                continue;
            };
            out.handoffs.push((
                player,
                to,
                Box::new(Arrival {
                    entity,
                    sheet,
                    pos,
                    facing,
                    destination,
                    step_wait,
                    vigor,
                    mp,
                    left_from: None,
                    party: None,
                }),
            ));
        }
    }

    /// The picture of every entity near each neighbor's side of the seam,
    /// sorted by id, for the binary to send as ghosts (it sends only the
    /// difference).
    pub(crate) fn border(&self, out: &mut TickOutput) {
        for (zone, rect) in &self.seam.neighbors {
            let area = rect.grow(SEAM_BAND);
            let mut list: Vec<EntityState> = self
                .by_id
                .iter()
                .filter_map(|(id, h)| {
                    let pos = self.world.get::<&Body>(*h).ok()?.pos;
                    area.contains(pos).then_some(())?;
                    self.entity_state(*id)
                })
                .collect();
            list.sort_by_key(|s| s.id);
            out.border.push((*zone, list));
        }
    }

    /// A player is fallen or not: whether the handed-over walk may resume.
    pub(crate) fn is_player(&self, h: hecs::Entity) -> bool {
        self.world.get::<&PlayerComp>(h).is_ok()
    }
}
