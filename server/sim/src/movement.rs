//! Walks one tile every `Direction::step_ticks` (4 ticks, 7 along the
//! screen's horizontal) along the planned path. One being per
//! tile: a step into an occupied tile waits, re-plans once around
//! the occupied neighbors, and gives up after `BLOCKED_GIVE_UP_TICKS`. A
//! step into a tile that now blocks on the map cancels the walk, and so
//! does a step off the zone's own tiles that the seam does not allow. A
//! neighbor's ghost does not hold its tile (it may be a tick or two old):
//! the two sides of a gate on the seam do not queue for each other.

use crate::Zone;
use crate::zone::{BLOCKED_GIVE_UP_TICKS, Body, PATH_MAX_NODES, Walk};
use base::TilePos;
use world::SpatialGrid;

/// Closed for planning: an occupied tile next to the walker, except the
/// destination itself (walking up to someone stops beside them).
pub(crate) fn occupied_near(grid: &SpatialGrid, from: TilePos, to: TilePos, t: TilePos) -> bool {
    t != to && t.distance(from) <= 1 && grid.occupied(t)
}

pub(crate) fn run(z: &mut Zone) {
    let now = z.tick.0;
    for &handle in z.by_id.values() {
        let player = z.is_player(handle);
        let next_ok = {
            let Ok(walk) = z.world.get::<&Walk>(handle) else {
                continue;
            };
            walk.steps.front().is_none_or(|n| z.may_step(*n, player))
        };
        let Ok((body, walk)) = z.world.query_one_mut::<(&mut Body, &mut Walk)>(handle) else {
            continue;
        };
        // Phase 10: monsters stay on the zone's own tiles; players walk on
        // a neighbor's only while it is up (`seam`).
        if !next_ok {
            walk.steps.clear();
            continue;
        }
        if now < walk.ready {
            continue;
        }
        let Some(&next) = walk.steps.front() else {
            continue;
        };
        let Some(dir) = body.pos.direction_to(next) else {
            walk.steps.clear();
            continue;
        };
        if body.pos.distance(next) != 1 || z.map.blocks_movement(next) {
            walk.steps.clear();
            continue;
        }
        if z.grid.occupied(next) {
            let destination = walk.steps.back().copied().unwrap_or(next);
            if next == destination {
                walk.steps.clear();
                walk.blocked = 0;
                continue;
            }
            if walk.blocked == 0 {
                let (from, grid) = (body.pos, &z.grid);
                if let Some(steps) = world::path_avoiding(&z.map, from, destination, PATH_MAX_NODES, |t| {
                    occupied_near(grid, from, destination, t)
                }) {
                    walk.steps = steps.into();
                }
            }
            walk.blocked = walk.blocked.saturating_add(1);
            if walk.blocked > BLOCKED_GIVE_UP_TICKS {
                walk.steps.clear();
                walk.blocked = 0;
            }
            continue;
        }
        walk.blocked = 0;
        walk.steps.pop_front();
        z.grid.moved(body.id, body.pos, next);
        body.pos = next;
        body.facing = dir;
        body.moved_tick = now;
        walk.ready = now.saturating_add(dir.step_ticks());
    }
}
