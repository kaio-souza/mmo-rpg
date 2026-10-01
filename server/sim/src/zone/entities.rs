//! Entities and the zone's indexes.
//!
//! The ECS world answers "what components does this handle have?", not
//! "which handle is entity 7?" or "which handle is player 3?". Those two
//! questions run on every tick, so the zone keeps two indexes beside the
//! world: `by_id` ([`EntityId`] → handle) and `by_player` ([`PlayerId`] →
//! handle).
//!
//! Two indexes beside the world can drift out of it. Everything that puts
//! an entity in or takes one out goes through [`Zone::insert_entity`] and
//! [`Zone::remove_entity`], which touch the world, the spatial grid and
//! both indexes together, so no caller can update one and forget another.
//! [`Zone::index_error`] states the invariant; `Zone::tick` checks it in
//! debug builds and the tests check it after entering, fighting, dying,
//! reloading and leaving.

use super::*;
use crate::fight::Fighter;

impl Zone {
    /// Puts a new entity in the world, in the grid and in every index.
    /// `bundle` must carry a [`Body`] with `id`; a bundle with a
    /// [`PlayerComp`] also enters `by_player`.
    pub(crate) fn insert_entity<B>(&mut self, id: EntityId, pos: TilePos, bundle: B) -> hecs::Entity
    where
        B: hecs::DynamicBundle,
    {
        let handle = self.world.spawn(bundle);
        self.grid.insert(id, pos);
        self.by_id.insert(id, handle);
        if let Some(player) = self.world.get::<&PlayerComp>(handle).ok().map(|p| p.player) {
            self.by_player.insert(player, handle);
        }
        handle
    }

    /// Takes the entity out of the world, the grid and every index.
    /// `false` when no entity of the zone has that id.
    pub(crate) fn remove_entity(&mut self, id: EntityId) -> bool {
        let Some(handle) = self.by_id.remove(&id) else {
            return false;
        };
        if let Ok(pos) = self.world.get::<&Body>(handle).map(|b| b.pos) {
            self.grid.remove(id, pos);
        }
        if let Ok(player) = self.world.get::<&PlayerComp>(handle).map(|p| p.player) {
            self.by_player.remove(&player);
        }
        let _ = self.world.despawn(handle);
        true
    }

    /// The id of an entity by its handle.
    pub(crate) fn id_of(&self, handle: hecs::Entity) -> Option<EntityId> {
        self.world.get::<&Body>(handle).ok().map(|b| b.id)
    }

    /// What is wrong between the world and the indexes, if anything. Four
    /// rules, both ways: every index entry points at a live entity whose
    /// own component agrees with the key, and every entity with a `Body`
    /// or a `PlayerComp` is in the matching index.
    pub(crate) fn index_error(&self) -> Option<String> {
        for (&id, &handle) in &self.by_id {
            match self.world.get::<&Body>(handle) {
                Ok(b) if b.id == id => {}
                Ok(b) => return Some(format!("by_id[{}] holds the body of {}", id.get(), b.id.get())),
                Err(_) => return Some(format!("by_id[{}] has no body", id.get())),
            }
        }
        for (&player, &handle) in &self.by_player {
            match self.world.get::<&PlayerComp>(handle) {
                Ok(p) if p.player == player => {}
                Ok(p) => {
                    return Some(format!(
                        "by_player[{}] holds player {}",
                        player.get(),
                        p.player.get()
                    ));
                }
                Err(_) => return Some(format!("by_player[{}] is not a player", player.get())),
            }
            match self.world.get::<&Body>(handle).map(|b| b.id) {
                Ok(id) if self.by_id.get(&id) == Some(&handle) => {}
                Ok(id) => return Some(format!("player {} is not by_id[{}]", player.get(), id.get())),
                Err(_) => return Some(format!("player {} has no body", player.get())),
            }
        }
        for (handle, body) in self.world.query::<&Body>().iter() {
            if self.by_id.get(&body.id) != Some(&handle) {
                return Some(format!("entity {} is not in by_id", body.id.get()));
            }
        }
        for (handle, p) in self.world.query::<&PlayerComp>().iter() {
            if self.by_player.get(&p.player) != Some(&handle) {
                return Some(format!("player {} is not in by_player", p.player.get()));
            }
        }
        None
    }

    /// Entity id and position of `id`, when it is in the zone.
    pub(crate) fn body_of(&self, id: EntityId) -> Option<(hecs::Entity, TilePos)> {
        let h = *self.by_id.get(&id)?;
        let pos = self.world.get::<&Body>(h).ok()?.pos;
        Some((h, pos))
    }

    /// Whether the entity is up (not fallen).
    pub(crate) fn standing(&self, h: hecs::Entity) -> bool {
        self.world.get::<&Fighter>(h).is_ok_and(|f| f.fallen_until == 0)
    }
}
