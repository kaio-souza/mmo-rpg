//! Content definitions, loading of `content/` and validation.
//!
//! Phase 3 covers texts, zones,
//! NPCs, dialogues (with conditions and effects), items, quests and script
//! sources.
//!
//! The only IO in `shared/` lives in [`load`]; everything else works on text
//! already in memory.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod appearance;
mod assets;
mod catalog;
mod craft;
mod dungeon;
mod economy;
#[cfg(feature = "editor")]
pub mod editor;
mod emote;
mod formulas;
mod goal;
mod item;
mod load;
mod locales;
mod npc;
mod problem;
mod progress;
mod quest;
mod rank;
mod rules;
mod sound;
mod tame;
mod zone;

pub use appearance::{AppearanceDef, LOOK_LISTS, LookOption, MAX_LOOK_OPTIONS, look_name_key};
pub use assets::AssetNames;
pub use catalog::{Catalog, ContentProblem, ValidatedCatalog};
pub use craft::{
    MAX_MATERIALS, Material, NodeDef, NodeSpawn, RecipeDef, node_name_key, recipe_desc_key, recipe_name_key,
};
pub use dungeon::{DungeonDef, Lockout, MAX_DUNGEON_PARTY, dungeon_name_key};
pub use economy::{
    LootTableDef, PRICE_PERMILLE, Purse, ShopDef, ShopStock, buy_price, repair_price, sell_price,
};
pub use emote::{DEFAULT_EMOTE_COOLDOWN_TICKS, EmoteDef, emote_name_key};
pub use formulas::{Attributes, CombatFormulas, EconomyFormulas, Formulas, ProgressionFormulas};
pub use goal::{GoalDef, goal_desc_key, goal_name_key};
pub use item::{
    Binding, Equip, HiddenRoll, ItemDef, ItemLight, MAX_LIGHT_RADIUS, MAX_STACK, UseEffect, item_desc_key,
    item_name_key,
};
pub use load::{ContentErrors, from_text, load, read_sounds};
pub use locales::{LocaleDef, parse_locales};
pub use npc::{DialogueDef, DialogueNode, DialogueOption, MAX_OPTIONS, NpcDef, NpcSpawn, name_key};
pub use problem::{ProblemPart, TEMPLATES as PROBLEM_TEMPLATES, problem_parts};
pub use progress::{
    Announce, Buff, BuffStat, COUNTER_FAMILIES, COUNTER_KEYS, ClassDef, ClassReq, ClassSpecial, LootDef,
    MAX_AREA, MAX_STATUS_TICKS, MonsterDef, MonsterPhase, PercentBonus, ProficiencyDef, ProficiencyKind,
    SPECIAL_MAX, SpecialAttributeDef, Status, StatusKind, TechniqueDef, TitleBonus, TitleDef, TitleTrigger,
    UNARMED, Unique, class_name_key, counter_key_kind, title_name_key,
};
pub use quest::{MAX_OBJECTIVES, Objective, QuestDef, Rank, Reward, quest_desc_key, quest_name_key};
pub use rank::{MAX_RANKING_SIZE, RankBy, RankingDef, ranking_name_key};
pub use rules::{AFFINITY_MAX, Condition, Effect, KARMA_MAX, MAX_EFFECTS, QuestStatus};
pub use sound::{
    DEFAULT_MUSIC_FADE, EventSounds, FULL_VOLUME, MAX_SOUND_VARIANTS, MUSIC_SCREENS, SOUND_LICENSES,
    SOUND_SAMPLE_RATE, ScreenMusic, SoundConfig, SoundEvent, SoundFile, ZoneMusic,
    valid_file as valid_sound_file,
};
pub use tame::{Tame, TameReq};
pub use zone::{
    DENSE_SPAWN, HANDOFF_DEPTH, InstanceRule, MAX_FLOOR, MAX_VIEW_RADIUS, MAX_ZONE_SIDE, Moment,
    MonsterSpawn, PortalDef, Rect, RegionDef, RegionKind, SEAM_BAND, TileDef, Weather, WeatherChance,
    ZoneDef, in_workshop, neighbors, region_of, weather_at, zone_at,
};
