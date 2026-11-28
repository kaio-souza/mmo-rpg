//! Base types shared by every crate: typed IDs, positions, directions, ticks,
//! fixed point numbers, bounded strings and text references.
//!
//! No game rules, no IO, no clock.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod bounded;
mod fixed;
mod hash;
mod id;
mod item;
mod locale;
mod rng;
mod sound;
mod space;
mod stats;
mod text;
mod time;

pub use bounded::{AccountName, BoundedString, BoundedStringError, CharacterName, ChatText};
pub use fixed::{Fixed, ParseFixedError};
pub use hash::{Fnv64, StateHash};
pub use id::{
    AccountId, CharacterId, ClassId, DialogueId, EmoteId, EntityId, ItemId, LootTableId, MonsterId, NodeId,
    NpcId, PlayerId, ProficiencyId, QuestId, RecipeId, RegionId, ShopId, SpriteId, TechniqueId, TitleId,
    ZoneId,
};
pub use item::{IdBlock, ItemInstanceId, ItemKind, ItemPower, Rarity};
pub use locale::{Locale, ParseLocaleError};
pub use rng::Rng64;
pub use sound::SoundEvent;
pub use space::{Direction, TilePos};
pub use stats::{Attribute, Look, Resource};
pub use text::{Arg, TextId, TextRef};
pub use time::{STEP_TICKS, TICKS_PER_SECOND, Tick, WIDE_STEP_TICKS, hour_roll, world_hour};
