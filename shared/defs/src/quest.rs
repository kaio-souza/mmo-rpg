//! Quests (phase 3: no combat objectives yet).
//!
//! ```ron
//! // content/quests/first_steps.ron
//! (
//!     rank: F,
//!     repeatable: false,
//!     objectives: [Talk(npc: "herbalist"), Item(item: "glow_herb", qty: 3)],
//!     rewards: [Coins(150), Item(item: "travel_bread", qty: 2)],
//! )
//! ```
//!
//! A reward may also be `Technique("<id>")`: the quest teaches it.
//!
//! Name and description are always `quest.<id>.name` and `quest.<id>.desc`.
//! New objective kinds (kill, reach a region...) are appended as variants.

use crate::npc::valid_id;
use base::{ItemId, NpcId, QuestId, RecipeId, TechniqueId, TextId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Objectives of one quest (the protocol and the save keep one flag each).
pub const MAX_OBJECTIVES: usize = 8;

/// Adventurer's Guild rank, F lowest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum Rank {
    #[default]
    F,
    E,
    D,
    C,
    B,
    A,
    S,
    SS,
    SSS,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Objective {
    /// Hold `qty`; taken when the quest completes.
    Item { item: ItemId, qty: u32 },
    /// Talk to this NPC while the quest is active.
    Talk { npc: NpcId },
    /// Defeat `qty` of this monster while the quest is active (a kill
    /// counts for the players who earn its XP).
    Kill { monster: base::MonsterId, qty: u32 },
    /// Stand within `radius` tiles of `pos` (global) while the quest is
    /// active: travel.
    Reach { pos: base::TilePos, radius: u8 },
    /// Craft `qty` of this item while the quest is active.
    Craft { item: ItemId, qty: u32 },
}

impl Objective {
    /// The count this objective needs (`Reach` and `Talk`: 1).
    pub fn needed(&self) -> u32 {
        match self {
            Objective::Item { qty, .. } | Objective::Kill { qty, .. } | Objective::Craft { qty, .. } => *qty,
            Objective::Talk { .. } | Objective::Reach { .. } => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reward {
    Item {
        item: ItemId,
        qty: u32,
    },
    /// Copper.
    Coins(u64),
    /// Teaches a technique at level 1, like a dialogue's `LearnTechnique`
    /// (already known is not an error). It lives in the sheet, so the
    /// simulation gives it where it holds the sheet, not in `PlayerData`.
    Technique(TechniqueId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestDef {
    /// File stem; what saves store.
    pub id: String,
    pub rank: Rank,
    /// May start again after it completes.
    pub repeatable: bool,
    /// Alpha: a repeatable quest may start again only this long (wall clock
    /// seconds) after it was completed; 0: at once.
    pub cooldown_secs: u32,
    pub objectives: Vec<Objective>,
    pub rewards: Vec<Reward>,
    /// `quest.<id>.name`.
    pub name: TextId,
    /// `quest.<id>.desc`.
    pub description: TextId,
}

impl QuestDef {
    /// Techniques the quest teaches on completion.
    pub fn techniques(&self) -> impl Iterator<Item = TechniqueId> + '_ {
        self.rewards.iter().filter_map(|r| match r {
            Reward::Technique(t) => Some(*t),
            _ => None,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestFile {
    #[serde(default)]
    rank: Rank,
    #[serde(default)]
    repeatable: bool,
    /// Seconds before a repeatable quest may start again (0: at once).
    #[serde(default, skip_serializing_if = "is_zero")]
    cooldown_secs: u32,
    objectives: Vec<ObjectiveFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rewards: Vec<RewardFile>,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub(crate) enum ObjectiveFile {
    Item {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
        item: String,
        qty: u32,
    },
    Talk {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::NpcRef"))]
        npc: String,
    },
    /// Defeat `qty` of a monster while the quest is active.
    Kill {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::MonsterRef"))]
        monster: String,
        qty: u32,
    },
    /// Reach a place: within `radius` tiles of `pos`.
    Reach {
        pos: base::TilePos,
        #[serde(default = "two")]
        radius: u8,
    },
    /// Craft `qty` of an item while the quest is active.
    Craft {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
        item: String,
        qty: u32,
    },
}

fn two() -> u8 {
    2
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub(crate) enum RewardFile {
    Item {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
        item: String,
        qty: u32,
    },
    Coins(u64),
    Technique(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::TechniqueRef"))] String),
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Content ids already numbered, so references resolve while parsing.
pub(crate) struct Names<'a> {
    pub items: &'a BTreeMap<String, ItemId>,
    pub npcs: &'a BTreeMap<String, NpcId>,
    pub quests: &'a BTreeMap<String, QuestId>,
    /// Phase 8: recipes a dialogue may teach.
    pub recipes: &'a BTreeMap<String, RecipeId>,
    /// Techniques a dialogue may teach (first magic by an NPC).
    pub techniques: &'a BTreeMap<String, TechniqueId>,
    /// Monsters a quest may ask to defeat.
    pub monsters: &'a BTreeMap<String, base::MonsterId>,
    /// Server goals a dialogue may wait for (`goals/`).
    pub goals: &'a BTreeMap<String, String>,
}

pub(crate) fn parse_quest(
    id: &str,
    text: &str,
    names: &Names<'_>,
    name: TextId,
    description: TextId,
) -> Result<QuestDef, Vec<String>> {
    if !valid_id(id) {
        return Err(vec![format!("quest id `{id}` must be [a-z0-9_]")]);
    }
    let file: QuestFile = ron::from_str(text).map_err(|e| vec![format!("{id}.ron: {e}")])?;
    let mut problems = Vec::new();
    if file.cooldown_secs > 0 && !file.repeatable {
        problems.push("cooldown_secs needs repeatable: true".to_owned());
    }
    if file.objectives.is_empty() || file.objectives.len() > MAX_OBJECTIVES {
        problems.push(format!("needs 1 to {MAX_OBJECTIVES} objectives"));
    }
    let item = |problems: &mut Vec<String>, item: &str, qty: u32| -> Option<ItemId> {
        if qty == 0 {
            problems.push(format!("item `{item}`: qty must be at least 1"));
        }
        let found = names.items.get(item).copied();
        if found.is_none() {
            problems.push(format!("item `{item}` not found"));
        }
        found
    };
    let mut objectives = Vec::with_capacity(file.objectives.len());
    for o in &file.objectives {
        match o {
            ObjectiveFile::Item { item: i, qty } => {
                if let Some(it) = item(&mut problems, i, *qty) {
                    objectives.push(Objective::Item { item: it, qty: *qty });
                }
            }
            ObjectiveFile::Talk { npc } => match names.npcs.get(npc) {
                Some(n) => objectives.push(Objective::Talk { npc: *n }),
                None => problems.push(format!("npc `{npc}` not found")),
            },
            ObjectiveFile::Kill { monster, qty } => match names.monsters.get(monster) {
                Some(_) if *qty == 0 => problems.push(format!("monster `{monster}`: qty must be at least 1")),
                Some(m) => objectives.push(Objective::Kill {
                    monster: *m,
                    qty: *qty,
                }),
                None => problems.push(format!("monster `{monster}` not found")),
            },
            // Whether `pos` lies in a zone is checked with the zones.
            ObjectiveFile::Reach { pos, radius } => objectives.push(Objective::Reach {
                pos: *pos,
                radius: *radius,
            }),
            ObjectiveFile::Craft { item: i, qty } => {
                if let Some(it) = item(&mut problems, i, *qty) {
                    objectives.push(Objective::Craft { item: it, qty: *qty });
                }
            }
        }
    }
    let mut rewards = Vec::with_capacity(file.rewards.len());
    for r in &file.rewards {
        match r {
            RewardFile::Item { item: i, qty } => {
                if let Some(it) = item(&mut problems, i, *qty) {
                    rewards.push(Reward::Item { item: it, qty: *qty });
                }
            }
            RewardFile::Coins(n) => rewards.push(Reward::Coins(*n)),
            RewardFile::Technique(t) => match names.techniques.get(t) {
                Some(t) => rewards.push(Reward::Technique(*t)),
                None => problems.push(format!("technique `{t}` not found")),
            },
        }
    }
    if problems.is_empty() {
        Ok(QuestDef {
            id: id.to_owned(),
            rank: file.rank,
            repeatable: file.repeatable,
            cooldown_secs: file.cooldown_secs,
            objectives,
            rewards,
            name,
            description,
        })
    } else {
        Err(problems)
    }
}

pub fn quest_name_key(id: &str) -> String {
    format!("quest.{id}.name")
}

pub fn quest_desc_key(id: &str) -> String {
    format!("quest.{id}.desc")
}
