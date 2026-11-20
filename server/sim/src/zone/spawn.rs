//! Where a new entity lands: the NPCs of `npcs.ron` at zone start, and
//! the entry tile of a player or monster.
//!
//! The tile comes from a hash of the zone seed and the entity id, never
//! from the generator's state, so two runs place the same entity on the
//! same tile.

use super::*;

impl Zone {
    /// Places every NPC of `npcs.ron`; ids are assigned in file order.
    /// An NPC with a shop (phase 7) brings its stock along.
    pub(super) fn spawn_npcs(&mut self, def: &defs::ZoneDef) {
        let content = self.content.clone();
        let now = self.tick.0;
        for spawn in &def.npcs {
            let Some((npc_id, npc)) = content.catalog.npc_by_name(&spawn.npc) else {
                continue;
            };
            let id = EntityId::new(self.next_id);
            self.next_id = self.next_id.saturating_add(1);
            let handle = self.insert_entity(
                id,
                spawn.pos,
                (
                    Body {
                        id,
                        pos: spawn.pos,
                        facing: base::Direction::S,
                        moved_tick: 0,
                        status_tick: 0,
                    },
                    Walk::default(),
                    NpcComp {
                        npc: npc_id,
                        name: npc.name,
                        dialogue: npc.dialogue,
                    },
                ),
            );
            if let Some((shop, sdef)) = npc.shop.and_then(|s| Some((s, content.catalog.shop(s)?))) {
                let _ = self.world.insert_one(handle, ShopComp::new(shop, sdef, now));
            }
        }
    }

    /// Places every resource node of `nodes.ron` (phase 8). A node stands
    /// still and has no fighter: it is gathered, never fought.
    pub(super) fn spawn_nodes(&mut self, def: &defs::ZoneDef) {
        let content = self.content.clone();
        for spawn in &def.nodes {
            let Some((node_id, _)) = content.catalog.node_by_name(&spawn.node) else {
                continue;
            };
            let id = EntityId::new(self.next_id);
            self.next_id = self.next_id.saturating_add(1);
            self.insert_entity(
                id,
                spawn.pos,
                (
                    Body {
                        id,
                        pos: spawn.pos,
                        facing: base::Direction::S,
                        moved_tick: 0,
                        status_tick: 0,
                    },
                    Walk::default(),
                    NodeComp::new(node_id),
                ),
            );
        }
    }

    /// A free tile up to `spawn_radius` from the spawn, picked from a hash of
    /// the seed and the entity id (deterministic, no generator state).
    pub(crate) fn spawn_point(&self, id: EntityId) -> TilePos {
        let r = i64::from(self.spawn_radius);
        if r == 0 {
            return self.nearest_free(self.spawn);
        }
        let span = u64::try_from(r.saturating_mul(2).saturating_add(1)).unwrap_or(1);
        let offset = |v: u64| {
            i64::try_from(v.checked_rem(span).unwrap_or(0))
                .unwrap_or(0)
                .saturating_sub(r)
        };
        for attempt in 0..16u64 {
            let mut h = Fnv64::new();
            h.write_u64(self.seed);
            h.write_u64(u64::from(id.get()));
            h.write_u64(attempt);
            let v = h.finish();
            let dx = offset(v);
            let dy = offset(v.checked_div(span).unwrap_or(0));
            let at = |o: i32, d: i64| i32::try_from(i64::from(o).saturating_add(d)).unwrap_or(o);
            let p = TilePos::new(at(self.spawn.x, dx), at(self.spawn.y, dy));
            if self.free(p) {
                return p;
            }
        }
        self.nearest_free(self.spawn)
    }

    /// Walkable, nobody on it (one per tile) and the zone's own
    /// (phase 10: nothing is placed on a neighbor's side of the seam; an
    /// arrival put there would bounce straight back).
    pub(crate) fn free(&self, p: TilePos) -> bool {
        !self.map.blocks_movement(p) && !self.grid.occupied(p) && self.seam.owns(p)
    }

    /// First free tile in rings around `p` (fixed order), or `p` itself.
    pub(crate) fn nearest_free(&self, p: TilePos) -> TilePos {
        for r in 0..=SPAWN_SEARCH_RADIUS {
            for dy in r.saturating_neg()..=r {
                for dx in r.saturating_neg()..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let q = TilePos::new(p.x.saturating_add(dx), p.y.saturating_add(dy));
                    if self.free(q) {
                        return q;
                    }
                }
            }
        }
        p
    }
}
