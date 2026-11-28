//! Simulation of one zone: ECS world, systems in a fixed order, views per
//! player. Deterministic and without IO: commands in, views out.
//!
//! Players enter, walk (A* tile
//! by tile), see each other (radius + line of sight), talk to NPCs, carry
//! items and do quests (phase 3).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
#![deny(clippy::float_arithmetic)]
// Overflow and division by zero never panic in a tick; tests may use plain arithmetic.
#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

mod actions;
mod craft;
mod dialogue;
mod escrow;
mod fight;
mod gear;
mod goal;
mod growth;
mod interest;
mod movement;
mod player;
mod portal;
mod replay;
mod scratch;
mod seam;
mod server_goal;
mod shop;
mod social;
mod status;
mod types;
mod zone;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_dungeon;
#[cfg(test)]
mod tests_escrow;
#[cfg(test)]
mod tests_fight;
#[cfg(test)]
mod tests_pace;
#[cfg(test)]
mod tests_seam;
#[cfg(test)]
mod tests_social;
#[cfg(test)]
mod tests_trade;

pub use gear::LOW_ITEM_IDS;
pub use player::BACKPACK_SLOTS;
pub use replay::{ReplayLog, SEGMENT_TICKS, TickRecord, ZoneState};
pub use seam::{Arrival, ENTITY_ID_BITS, GHOST_TIMEOUT_TICKS, GhostDiff, world_map};
pub use social::{INVITE_TICKS, MAX_PARTY, SHARE_RADIUS, TRADE_RADIUS, TRADE_TICKS};
pub use types::{
    CharacterSheet, ClearOut, CoinFlow, CombatOut, Command, DeliveredOut, DialogueOut, EntityKind,
    EntityLabel, EntityState, EquipSlot, EscrowOut, EscrowParcel, InstanceView, InventoryView, InviteKind,
    InviteOut, ItemPlace, LoadLevel, ObjectiveView, PartyView, QuestMarker, QuestView, Refusal, SavedItem,
    SavedPower, SavedProgress, SavedQuest, SavedStack, ShopLineView, ShopView, StatsView, TickInput,
    TickOutput, TradeView, View,
};
pub use zone::{
    BLOCKED_GIVE_UP_TICKS, MAX_MOVE_DISTANCE, PATH_MAX_NODES, STEP_TICKS, TALK_RADIUS, Zone, ZoneError,
    zone_map,
};

/// Validated content: what a zone runs on. Shared (`Arc`) by every zone;
/// replaced as a whole on reload. No scripts.
#[derive(Debug)]
pub struct Content {
    pub catalog: defs::ValidatedCatalog,
}

impl Content {
    pub fn new(catalog: defs::ValidatedCatalog) -> Content {
        Content { catalog }
    }
}
