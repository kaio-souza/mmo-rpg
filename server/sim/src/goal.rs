//! Where a quest leads next, so the player is never lost. Pure:
//! the catalog's placements (every zone, global coordinates) and the
//! quest's progress in, one tile out.

use base::{ItemId, NpcId, QuestId, TilePos};
use defs::{Effect, Objective, ValidatedCatalog};

/// The tile the quest asks for next, nearest to `from`: the NPC of the
/// first `Talk` not done, else a place that yields the first item still
/// missing (a monster that drops it, a node that gives it), else the NPC
/// that takes the quest in. `None` when nothing in the world fits.
pub(crate) fn quest_goal(
    cat: &ValidatedCatalog,
    quest: QuestId,
    talked: impl Fn(usize) -> bool,
    have: impl Fn(ItemId) -> u64,
    from: TilePos,
) -> Option<TilePos> {
    let def = cat.quest(quest)?;
    for (i, o) in def.objectives.iter().enumerate() {
        match o {
            Objective::Talk { npc } if !talked(i) => return npc_place(cat, from, |id| id == *npc),
            Objective::Item { item, qty } if have(*item) < u64::from(*qty) => {
                if let Some(p) = item_place(cat, *item, from) {
                    return Some(p);
                }
            }
            _ => {}
        }
    }
    // Everything in hand: back to whoever takes it in.
    npc_place(cat, from, |id| {
        cat.npc(id)
            .and_then(|n| n.dialogue)
            .and_then(|d| cat.dialogue(d))
            .is_some_and(|d| {
                d.nodes
                    .iter()
                    .flat_map(|n| n.options.iter())
                    .flat_map(|o| o.effects.iter())
                    .any(|e| matches!(e, Effect::CompleteQuest(q) if *q == quest))
            })
    })
}

fn nearest(from: TilePos, places: impl Iterator<Item = TilePos>) -> Option<TilePos> {
    places.min_by_key(|p| (from.distance(*p), p.y, p.x))
}

fn npc_place(cat: &ValidatedCatalog, from: TilePos, wanted: impl Fn(NpcId) -> bool) -> Option<TilePos> {
    nearest(
        from,
        cat.zones()
            .flat_map(|(_, z)| z.npcs.iter())
            .filter(|s| cat.npc_by_name(&s.npc).is_some_and(|(id, _)| wanted(id)))
            .map(|s| s.pos),
    )
}

/// Spawns of monsters that drop `item` and nodes that yield it.
fn item_place(cat: &ValidatedCatalog, item: ItemId, from: TilePos) -> Option<TilePos> {
    let in_table = |t: Option<base::LootTableId>| {
        t.and_then(|t| cat.loot_table(t))
            .is_some_and(|t| t.entries.iter().any(|l| l.item == item))
    };
    let monsters = cat
        .zones()
        .flat_map(|(_, z)| z.monsters.iter())
        .filter(|s| {
            cat.monster_by_name(&s.monster)
                .is_some_and(|(_, m)| m.loot.iter().any(|l| l.item == item) || in_table(m.loot_table))
        })
        .map(|s| s.pos);
    let nodes = cat
        .zones()
        .flat_map(|(_, z)| z.nodes.iter())
        .filter(|s| {
            cat.node_by_name(&s.node)
                .is_some_and(|(_, n)| in_table(Some(n.loot)))
        })
        .map(|s| s.pos);
    nearest(from, monsters.chain(nodes))
}
