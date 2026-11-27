//! Portals and teleports: a player who steps on a portal tile
//! (`zone.ron`, `portals`) or picks a dialogue option with `Teleport` goes
//! to a tile of the world. In the same zone it is a jump; to another zone
//! it is a hand-off like crossing the seam (`TickOutput::handoffs`), so
//! the gateway moves the player and the next zone sends the whole view.

use crate::Zone;
use crate::fight::Fighter;
use crate::seam::Arrival;
use crate::types::TickOutput;
use crate::zone::{Body, Walk};
use base::{PlayerId, TilePos};

/// Players who stepped on a portal this tick, then the teleports asked by
/// dialogues, in that order.
pub(crate) fn run(z: &mut Zone, out: &mut TickOutput) {
    let now = z.tick.0;
    let mut moves: Vec<(PlayerId, TilePos)> = Vec::new();
    if let Some(def) = z.content.catalog.zone(z.zone_id())
        && !def.portals.is_empty()
    {
        for (player, h) in &z.by_player {
            let Ok(body) = z.world.get::<&Body>(*h) else {
                continue;
            };
            if body.moved_tick != now {
                continue;
            }
            if let Some(p) = def.portals.iter().find(|p| p.holds(body.pos)) {
                moves.push((*player, p.to));
            }
        }
    }
    moves.append(&mut z.teleports);
    for (player, to) in moves {
        teleport(z, player, to, out);
    }
}

/// Every player of the zone to `to` (an instance closing on them).
pub(crate) fn evict(z: &mut Zone, to: TilePos, out: &mut TickOutput) {
    let players: Vec<PlayerId> = z.by_player.keys().copied().collect();
    for p in players {
        teleport(z, p, to, out);
    }
}

/// Sends `player` to `to`: a jump in this zone, or a hand-off to the zone
/// whose map holds it. A tile in no zone does nothing (content checks it).
pub(crate) fn teleport(z: &mut Zone, player: PlayerId, to: TilePos, out: &mut TickOutput) {
    let content = z.content.clone();
    let cat = &content.catalog;
    let Some(&h) = z.by_player.get(&player) else {
        return;
    };
    let Some((target, _)) = cat.zones().find(|(_, d)| d.tile_at(to).is_some()) else {
        return;
    };
    if target == z.zone_id() && z.seam.owns(to) {
        let dest = z.nearest_free(to);
        let now = z.tick.0;
        if let Ok((body, walk)) = z.world.query_one_mut::<(&mut Body, &mut Walk)>(h) {
            z.grid.moved(body.id, body.pos, dest);
            body.pos = dest;
            body.moved_tick = now;
            walk.steps.clear();
            walk.blocked = 0;
        }
        if let Ok(f) = z.world.query_one_mut::<&mut Fighter>(h) {
            f.target = None;
            f.retaliating = false;
        }
        return;
    }
    // A dungeon asks for a level and a group size at the door.
    if let Some(d) = cat.dungeon_of_zone(target) {
        let level = z
            .world
            .get::<&crate::zone::PlayerComp>(h)
            .map_or(0, |p| p.growth.sheet.level);
        let size = z.party_members(player).map_or(1, <[PlayerId]>::len);
        let size = u8::try_from(size).unwrap_or(u8::MAX);
        let fits = (d.level.0..=d.level.1).contains(&level) && (d.party.0..=d.party.1).contains(&size);
        if !fits {
            let int = |v: u8| base::Arg::Int(i64::from(v));
            let args = smallvec::smallvec![
                base::Arg::Ref(Box::new(base::TextRef::new(d.name))),
                int(d.level.0),
                int(d.level.1),
                int(d.party.0),
                int(d.party.1)
            ];
            z.tell(out, player, "system.dungeon_requirements", args);
            return;
        }
    }
    let (vigor, mp) = z.world.get::<&Fighter>(h).map_or((0, 0), |f| (f.vigor, f.mp));
    let facing = z.world.get::<&Body>(h).map_or(base::Direction::S, |b| b.facing);
    let party = z
        .party_members(player)
        .and_then(|m| m.first().copied())
        .unwrap_or(player);
    let Some((sheet, from, entity)) = z.take_player(player) else {
        return;
    };
    out.handoffs.push((
        player,
        target,
        Box::new(Arrival {
            entity,
            sheet,
            pos: to,
            facing,
            destination: None,
            step_wait: 0,
            vigor,
            mp,
            left_from: Some(from),
            party: Some(party),
        }),
    ));
}
