//! What each player sees: entities within the view radius and in line of
//! sight. Only the difference from what the player already knows is sent.
//!
//! The only parallel system: each player's view is computed from
//! a read-only copy of the bodies, in parallel; results are applied in
//! `PlayerId` order, so the output does not depend on thread timing.

use crate::Zone;
use crate::craft::NodeComp;
use crate::fight::{Fighter, MonsterComp};
use crate::gear::Gear;
use crate::types::{EntityKind, EntityLabel, EntityState, TickOutput, View};
use crate::zone::{Body, NpcComp, PlayerComp};
use base::{Direction, EntityId, PlayerId, TilePos};
use rayon::prelude::*;
use world::{SpatialGrid, TileMap};

/// Below this many players the work stays on the calling thread.
const PARALLEL_MIN_PLAYERS: usize = 64;

#[derive(Clone, Copy)]
pub(crate) struct Seen {
    id: EntityId,
    pos: TilePos,
    facing: Direction,
    moved: bool,
    /// HP or look changed this tick.
    status: bool,
}

/// One player's view job. The two buffers are reused tick after tick:
/// `known` is moved out of the player's component and back, and `visible`
/// is the slot's own buffer ([`crate::scratch`]).
pub(crate) struct Observer {
    player: PlayerId,
    handle: hecs::Entity,
    me: EntityId,
    pos: TilePos,
    known: Vec<EntityId>,
    visible: Vec<EntityId>,
    /// `visible` differs from `known`: the component takes the new set.
    changed: bool,
    /// Sight cache: who this slot saw last time, and from where. Whether a
    /// body is in sight depends only on the two tiles and the map, so a
    /// player who stayed put still sees a body that stayed put without a
    /// new line-of-sight walk. `seen` holds `(id, pos)` sorted by id;
    /// `seen_from` is the player, entity and tile it was taken for (the
    /// slot may hold another player next tick); the map reload clears the
    /// slots.
    seen: Vec<(EntityId, TilePos)>,
    seen_next: Vec<(EntityId, TilePos)>,
    seen_from: Option<(PlayerId, hecs::Entity, TilePos)>,
}

impl Default for Observer {
    fn default() -> Observer {
        Observer {
            player: PlayerId::new(0),
            handle: hecs::Entity::DANGLING,
            me: EntityId::new(0),
            pos: TilePos::new(0, 0),
            known: Vec::new(),
            visible: Vec::new(),
            changed: false,
            seen: Vec::new(),
            seen_next: Vec::new(),
            seen_from: None,
        }
    }
}

pub(crate) fn run(z: &mut Zone, out: &mut TickOutput) {
    let now = z.tick.0;
    // Every buffer of this system is reused between ticks; with a thousand
    // players the old code allocated two vectors per player per tick.
    let mut bodies = std::mem::take(&mut z.scratch.bodies);
    let mut observers = std::mem::take(&mut z.scratch.observers);
    let mut referenced = std::mem::take(&mut z.scratch.ids);

    // Dense, sorted copy of every body: lookups by binary search, no ECS access in the hot loop.
    bodies.clear();
    bodies.extend(z.world.query_mut::<&Body>().into_iter().map(|(_, b)| Seen {
        id: b.id,
        pos: b.pos,
        facing: b.facing,
        moved: b.moved_tick == now,
        status: b.status_tick == now,
    }));
    // Phase 10: the neighbors' entities near the seam are seen like the
    // zone's own (`seam`); a ghost that is a real entity here is skipped.
    let by_id = &z.by_id;
    bodies.extend(
        z.ghosts
            .by_id
            .iter()
            .filter(|(id, _)| !by_id.contains_key(id))
            .map(|(id, g)| Seen {
                id: *id,
                pos: g.state.pos,
                facing: g.state.direction,
                moved: g.moved_tick == now,
                status: g.status_tick == now,
            }),
    );
    bodies.sort_unstable_by_key(|s| s.id);
    let ghosts: Vec<(EntityId, TilePos)> = z
        .ghosts
        .by_id
        .iter()
        .filter(|(id, _)| !by_id.contains_key(id))
        .map(|(id, g)| (*id, g.state.pos))
        .collect();

    // One slot per player, in `PlayerId` order. A slot kept from an earlier
    // tick keeps its buffers; `known` is moved out of the component so
    // nothing is copied.
    let mut count = 0;
    z.with_players(|z, players| {
        for &(player, handle) in players {
            let Ok((pc, b)) = z.world.query_one_mut::<(&mut PlayerComp, &Body)>(handle) else {
                continue;
            };
            if observers.len() == count {
                observers.push(Observer::default());
            }
            let Some(o) = observers.get_mut(count) else {
                continue;
            };
            count = count.saturating_add(1);
            o.player = player;
            o.handle = handle;
            o.me = b.id;
            o.pos = b.pos;
            o.changed = false;
            // The set is moved out, not copied; the component holds an
            // empty buffer until the new set goes in at the end of the
            // system, and nothing reads it in between.
            std::mem::swap(&mut o.known, &mut pc.known);
            pc.known.clear();
        }
    });

    let ctx = Ctx {
        map: &z.map,
        grid: &z.grid,
        radius: z.view_radius,
        bodies: &bodies,
        ghosts: &ghosts,
    };
    // Only the first `count` slots hold a player this tick; the rest are
    // kept for their buffers.
    let views: Vec<View> = if count >= PARALLEL_MIN_PLAYERS {
        observers
            .par_iter_mut()
            .take(count)
            .map_init(Vec::new, |near, o| ctx.view_of(o, near))
            .collect()
    } else {
        let mut near = Vec::new();
        observers
            .iter_mut()
            .take(count)
            .map(|o| ctx.view_of(o, &mut near))
            .collect()
    };

    referenced.clear();
    for (o, view) in observers.iter_mut().take(count).zip(views) {
        // The new set goes into the component and the old one stays in the
        // slot, to be filled again next tick.
        if o.changed
            && let Ok(pc) = z.world.query_one_mut::<&mut PlayerComp>(o.handle)
        {
            std::mem::swap(&mut o.visible, &mut pc.known);
        } else if let Ok(pc) = z.world.query_one_mut::<&mut PlayerComp>(o.handle) {
            std::mem::swap(&mut o.known, &mut pc.known);
        }
        if !view.is_empty() {
            referenced.extend(view.entered.iter().copied());
            referenced.extend(view.updated.iter().copied());
            out.views.push((o.player, view));
        }
    }
    // Sorted and deduped: the same order a `BTreeSet` gave, without one.
    referenced.sort_unstable();
    referenced.dedup();

    for id in referenced.iter().copied() {
        if let Some(state) = z.entity_state(id) {
            out.entities.push(state);
        } else if let Some(g) = z.ghosts.by_id.get(&id) {
            out.entities.push(g.state.clone());
        }
    }

    bodies.clear();
    referenced.clear();
    z.scratch.bodies = bodies;
    z.scratch.ids = referenced;
    z.scratch.observers = observers;
}

struct Ctx<'a> {
    map: &'a TileMap,
    grid: &'a SpatialGrid,
    radius: u16,
    bodies: &'a [Seen],
    /// Ghosts of the neighbors (`seam`): not in the grid, few, checked one by one.
    ghosts: &'a [(EntityId, TilePos)],
}

impl Ctx<'_> {
    fn body(&self, id: EntityId) -> Option<&Seen> {
        let i = self.bodies.binary_search_by_key(&id, |s| s.id).ok()?;
        self.bodies.get(i)
    }

    /// The view change. Writes the visible set into the observer's own
    /// buffer and sets `changed` when it differs from what is known.
    fn view_of(&self, o: &mut Observer, near: &mut Vec<(EntityId, TilePos)>) -> View {
        near.clear();
        self.grid.neighbors(o.pos, self.radius, near);
        let r = u32::from(self.radius);
        near.extend(
            self.ghosts
                .iter()
                .filter(|(_, p)| o.pos.distance(*p) <= r)
                .copied(),
        );
        let from = (o.player, o.handle, o.pos);
        let seen = if o.seen_from == Some(from) {
            &o.seen[..]
        } else {
            &[][..]
        };
        let next = &mut o.seen_next;
        next.clear();
        next.extend(near.iter().copied().filter(|&(id, pos)| {
            id == o.me
                || o.pos.distance(pos) <= 1
                || seen
                    .binary_search_by_key(&id, |s| s.0)
                    .is_ok_and(|i| seen.get(i).is_some_and(|s| s.1 == pos))
                || world::line_of_sight(self.map, o.pos, pos)
        }));
        next.sort_unstable_by_key(|s| s.0);
        std::mem::swap(&mut o.seen, &mut o.seen_next);
        o.seen_from = Some(from);
        let visible = &mut o.visible;
        visible.clear();
        visible.extend(o.seen.iter().map(|s| s.0));

        let mut view = View::default();
        let known = &o.known;
        let (mut i, mut j) = (0, 0);
        loop {
            match (known.get(i), visible.get(j)) {
                (Some(k), Some(v)) if k == v => {
                    if let Some(s) = self.body(*v) {
                        if s.moved {
                            view.moved.push((s.id, s.pos, s.facing));
                        }
                        if s.status {
                            view.updated.push(s.id);
                        }
                    }
                    i = i.saturating_add(1);
                    j = j.saturating_add(1);
                }
                (Some(k), Some(v)) if k < v => {
                    view.left.push(*k);
                    i = i.saturating_add(1);
                }
                (Some(k), None) => {
                    view.left.push(*k);
                    i = i.saturating_add(1);
                }
                (_, Some(v)) => {
                    view.entered.push(*v);
                    j = j.saturating_add(1);
                }
                (None, None) => break,
            }
        }
        o.changed = *visible != *known;
        view
    }
}

impl Zone {
    /// What a client is told about an entity of this zone (also the ghost
    /// picture a neighbor gets).
    pub(crate) fn entity_state(&self, id: EntityId) -> Option<EntityState> {
        let h = *self.by_id.get(&id)?;
        let b = self.world.get::<&Body>(h).ok()?;
        let (health, level) = self
            .world
            .get::<&Fighter>(h)
            .map_or((100, 0), |f| (f.health(), f.level));
        let cat = &self.content.catalog;
        if let Ok(p) = self.world.get::<&PlayerComp>(h) {
            let (equipment, light) = self
                .world
                .get::<&Gear>(h)
                .map_or(((0, 0, 0), 0), |g| (g.visuals(cat), g.light(cat)));
            return Some(EntityState {
                id,
                kind: EntityKind::Player,
                pos: b.pos,
                direction: b.facing,
                appearance: 0,
                label: EntityLabel::Player(p.sheet.name.clone()),
                health,
                equipment,
                light,
                level,
                look: {
                    let [a, b, c, d] = &p.sheet.look;
                    cat.appearance().look_of([a, b, c, d])
                },
            });
        }
        if let Ok(n) = self.world.get::<&NpcComp>(h) {
            let appearance = cat.npc(n.npc).map_or(0, |def| cat.sprite_index(&def.sprite));
            return Some(EntityState {
                id,
                kind: EntityKind::Npc,
                pos: b.pos,
                direction: b.facing,
                appearance,
                label: EntityLabel::Npc(n.name),
                health,
                equipment: (0, 0, 0),
                light: 0,
                level,
                look: base::Look::default(),
            });
        }
        if let Ok(n) = self.world.get::<&NodeComp>(h) {
            let def = cat.node(n.node)?;
            return Some(EntityState {
                id,
                kind: EntityKind::Node,
                pos: b.pos,
                direction: b.facing,
                appearance: cat.sprite_index(&def.sprite),
                label: EntityLabel::Node(def.name),
                health: 100,
                equipment: (0, 0, 0),
                light: 0,
                level: 0,
                look: base::Look::default(),
            });
        }
        let m = self.world.get::<&MonsterComp>(h).ok()?;
        let def = cat.monster(m.monster)?;
        Some(EntityState {
            id,
            kind: EntityKind::Monster,
            pos: b.pos,
            direction: b.facing,
            appearance: cat.sprite_index(&def.sprite),
            label: EntityLabel::Monster(def.name),
            health,
            equipment: (0, 0, 0),
            light: 0,
            level,
            look: base::Look::default(),
        })
    }
}
