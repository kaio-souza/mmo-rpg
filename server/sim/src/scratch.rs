//! Buffers the tick loop reuses between ticks.
//!
//! The systems walk the zone's indexes (`by_id`, `by_player`) while the
//! ECS world is borrowed mutably, so they need an owned copy of the
//! entries. Allocating that copy inside the tick cost one allocation per
//! system per tick (and one per player in `interest`), which showed up in
//! the tick peaks with a thousand bots. The buffers live in the [`Zone`]
//! instead: they are cleared before every use, so they carry no state
//! from one tick to the next and never reach a snapshot or the state hash
//! ([ ]).
//!
//! Their contents always come from a `BTreeMap`, so the order is the same
//! on every machine.

use crate::interest::{Observer, Seen};
use crate::zone::Zone;
use base::{DialogueId, EntityId, NpcId, PlayerId};

/// Reused tick buffers. Every field is a scratch area, never state.
#[derive(Default)]
pub(crate) struct Scratch {
    /// `by_player` as a list, for systems that walk players.
    pub players: Vec<(PlayerId, hecs::Entity)>,
    /// Entity handles, for systems that only need the handle.
    pub handles: Vec<hecs::Entity>,
    /// Entity ids with their handle, for systems that walk `by_id`.
    pub entities: Vec<(EntityId, hecs::Entity)>,
    /// Entity ids alone (interest's referenced set, sorted and deduped).
    pub ids: Vec<EntityId>,
    /// What `flush_player_views` needs of each NPC to build quest markers.
    pub npcs: Vec<(EntityId, NpcId, Option<DialogueId>)>,
    /// Every body of the zone, sorted by id (interest).
    pub bodies: Vec<Seen>,
    /// One slot per player (interest); the slots keep their own buffers.
    pub observers: Vec<Observer>,
}

impl Zone {
    /// Runs `f` with `by_player` copied into a reused buffer, leaving the
    /// world free to be borrowed mutably inside `f`.
    pub(crate) fn with_players<R>(
        &mut self,
        f: impl FnOnce(&mut Zone, &[(PlayerId, hecs::Entity)]) -> R,
    ) -> R {
        let mut buf = std::mem::take(&mut self.scratch.players);
        buf.clear();
        buf.extend(self.by_player.iter().map(|(p, h)| (*p, *h)));
        let out = f(self, &buf);
        buf.clear();
        self.scratch.players = buf;
        out
    }

    /// Same as [`Zone::with_players`] with only the handles.
    pub(crate) fn with_player_handles<R>(&mut self, f: impl FnOnce(&mut Zone, &[hecs::Entity]) -> R) -> R {
        let mut buf = std::mem::take(&mut self.scratch.handles);
        buf.clear();
        buf.extend(self.by_player.values().copied());
        let out = f(self, &buf);
        buf.clear();
        self.scratch.handles = buf;
        out
    }

    /// Runs `f` with the entries of `by_id` that have component `C`,
    /// copied into a reused buffer.
    pub(crate) fn with_entities_of<C, R>(
        &mut self,
        f: impl FnOnce(&mut Zone, &[(EntityId, hecs::Entity)]) -> R,
    ) -> R
    where
        C: hecs::Component,
    {
        let mut buf = std::mem::take(&mut self.scratch.entities);
        buf.clear();
        buf.extend(
            self.by_id
                .iter()
                .filter(|(_, h)| self.world.get::<&C>(**h).is_ok())
                .map(|(id, h)| (*id, *h)),
        );
        let out = f(self, &buf);
        buf.clear();
        self.scratch.entities = buf;
        out
    }
}
