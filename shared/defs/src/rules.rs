//! Conditions and effects of dialogue options. Formats in
//! Kept generic: new kinds
//! (title, rank, class, time of day) are appended as variants.

use crate::quest::Names;
use base::{ItemId, QuestId, RecipeId, TechniqueId};
use serde::{Deserialize, Serialize};

/// Deepest nesting of `All`/`Any`/`Not`.
const MAX_DEPTH: u32 = 8;
/// Effects of one option.
pub const MAX_EFFECTS: usize = 16;
/// Affinity runs from -100 to 100.
pub const AFFINITY_MAX: i32 = 100;
/// Karma, the hidden morality of a character, runs from -1000 to 1000.
pub const KARMA_MAX: i32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum QuestStatus {
    NotStarted,
    Active,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    Item {
        item: ItemId,
        qty: u32,
    },
    Coins(u64),
    Quest {
        quest: QuestId,
        state: QuestStatus,
    },
    QuestReady(QuestId),
    /// Affinity with the NPC of the dialogue, at least this much.
    Affinity(i32),
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
    /// The player already knows this recipe (so a lesson is not offered twice).
    RecipeKnown(RecipeId),
    /// The player already knows this technique.
    TechniqueKnown(TechniqueId),
    /// The player's karma is at least this much (`Not` for at most).
    Karma(i32),
    /// This server goal (`goals/<id>.ron`) was reached.
    GoalDone(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    GiveItem {
        item: ItemId,
        qty: u32,
    },
    TakeItem {
        item: ItemId,
        qty: u32,
    },
    GiveCoins(u64),
    TakeCoins(u64),
    StartQuest(QuestId),
    CompleteQuest(QuestId),
    /// Added to the affinity with the NPC of the dialogue.
    Affinity(i32),
    /// Gives only what is missing for the player to hold `qty`
    /// (a gift that does not pile up).
    TopUpItem {
        item: ItemId,
        qty: u32,
    },
    /// Phase 8: teaches a recipe; already known is not an error.
    LearnRecipe(RecipeId),
    /// Teaches a technique at level 1 (the first magic comes
    /// from an NPC); already known is not an error.
    LearnTechnique(TechniqueId),
    /// Takes the player to this tile of the world (any zone) and ends the
    /// dialogue: a ferryman, a gate keeper.
    Teleport(base::TilePos),
    /// Added to the player's karma (hidden; clamped to the range).
    Karma(i32),
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub(crate) enum ConditionFile {
    Item {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
        item: String,
        qty: u32,
    },
    Coins(u64),
    Quest {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::QuestRef"))]
        quest: String,
        state: QuestStatus,
    },
    QuestReady(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::QuestRef"))] String),
    Affinity(i32),
    All(Vec<ConditionFile>),
    Any(Vec<ConditionFile>),
    Not(Box<ConditionFile>),
    RecipeKnown(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::RecipeRef"))] String),
    TechniqueKnown(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::TechniqueRef"))] String),
    Karma(i32),
    GoalDone(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::GoalRef"))] String),
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub(crate) enum EffectFile {
    GiveItem {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
        item: String,
        qty: u32,
    },
    TakeItem {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
        item: String,
        qty: u32,
    },
    GiveCoins(u64),
    TakeCoins(u64),
    StartQuest(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::QuestRef"))] String),
    CompleteQuest(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::QuestRef"))] String),
    Affinity(i32),
    TopUpItem {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
        item: String,
        qty: u32,
    },
    LearnRecipe(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::RecipeRef"))] String),
    LearnTechnique(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::TechniqueRef"))] String),
    /// Takes the player to this tile of the world and ends the dialogue.
    Teleport(base::TilePos),
    Karma(i32),
}

fn item(names: &Names<'_>, problems: &mut Vec<String>, id: &str, qty: u32) -> Option<ItemId> {
    if qty == 0 {
        problems.push(format!("item `{id}`: qty must be at least 1"));
    }
    let found = names.items.get(id).copied();
    if found.is_none() {
        problems.push(format!("item `{id}` not found"));
    }
    found
}

fn quest(names: &Names<'_>, problems: &mut Vec<String>, id: &str) -> Option<QuestId> {
    let found = names.quests.get(id).copied();
    if found.is_none() {
        problems.push(format!("quest `{id}` not found"));
    }
    found
}

fn affinity(problems: &mut Vec<String>, v: i32) {
    if !(-AFFINITY_MAX..=AFFINITY_MAX).contains(&v) {
        problems.push(format!("affinity {v} must be -{AFFINITY_MAX} to {AFFINITY_MAX}"));
    }
}

pub(crate) fn karma(problems: &mut Vec<String>, v: i32) {
    if !(-KARMA_MAX..=KARMA_MAX).contains(&v) {
        problems.push(format!("karma {v} must be -{KARMA_MAX} to {KARMA_MAX}"));
    }
}

pub(crate) fn condition(
    names: &Names<'_>,
    problems: &mut Vec<String>,
    file: &ConditionFile,
    depth: u32,
) -> Option<Condition> {
    if depth > MAX_DEPTH {
        problems.push(format!("condition nested deeper than {MAX_DEPTH}"));
        return None;
    }
    let list = |problems: &mut Vec<String>, v: &[ConditionFile]| -> Option<Vec<Condition>> {
        let out: Vec<Condition> = v
            .iter()
            .filter_map(|c| condition(names, problems, c, depth + 1))
            .collect();
        (out.len() == v.len()).then_some(out)
    };
    Some(match file {
        ConditionFile::Item { item: i, qty } => Condition::Item {
            item: item(names, problems, i, *qty)?,
            qty: *qty,
        },
        ConditionFile::Coins(n) => Condition::Coins(*n),
        ConditionFile::Quest { quest: q, state } => Condition::Quest {
            quest: quest(names, problems, q)?,
            state: *state,
        },
        ConditionFile::QuestReady(q) => Condition::QuestReady(quest(names, problems, q)?),
        ConditionFile::Affinity(v) => {
            affinity(problems, *v);
            Condition::Affinity(*v)
        }
        ConditionFile::Karma(v) => {
            karma(problems, *v);
            Condition::Karma(*v)
        }
        ConditionFile::GoalDone(g) => {
            if !names.goals.contains_key(g) {
                problems.push(format!("goal `{g}` not found"));
                return None;
            }
            Condition::GoalDone(g.clone())
        }
        ConditionFile::All(v) => Condition::All(list(problems, v)?),
        ConditionFile::Any(v) => Condition::Any(list(problems, v)?),
        ConditionFile::Not(c) => Condition::Not(Box::new(condition(names, problems, c, depth + 1)?)),
        ConditionFile::RecipeKnown(name) => match names.recipes.get(name) {
            Some(r) => Condition::RecipeKnown(*r),
            None => {
                problems.push(format!("recipe `{name}` not found"));
                return None;
            }
        },
        ConditionFile::TechniqueKnown(name) => match names.techniques.get(name) {
            Some(t) => Condition::TechniqueKnown(*t),
            None => {
                problems.push(format!("technique `{name}` not found"));
                return None;
            }
        },
    })
}

pub(crate) fn effect(names: &Names<'_>, problems: &mut Vec<String>, file: &EffectFile) -> Option<Effect> {
    Some(match file {
        EffectFile::GiveItem { item: i, qty } => Effect::GiveItem {
            item: item(names, problems, i, *qty)?,
            qty: *qty,
        },
        EffectFile::TakeItem { item: i, qty } => Effect::TakeItem {
            item: item(names, problems, i, *qty)?,
            qty: *qty,
        },
        EffectFile::GiveCoins(n) => Effect::GiveCoins(*n),
        EffectFile::TakeCoins(n) => Effect::TakeCoins(*n),
        EffectFile::StartQuest(q) => Effect::StartQuest(quest(names, problems, q)?),
        EffectFile::CompleteQuest(q) => Effect::CompleteQuest(quest(names, problems, q)?),
        EffectFile::Affinity(v) => {
            affinity(problems, *v);
            Effect::Affinity(*v)
        }
        EffectFile::TopUpItem { item: i, qty } => Effect::TopUpItem {
            item: item(names, problems, i, *qty)?,
            qty: *qty,
        },
        EffectFile::LearnRecipe(name) => match names.recipes.get(name) {
            Some(r) => Effect::LearnRecipe(*r),
            None => {
                problems.push(format!("recipe `{name}` not found"));
                return None;
            }
        },
        EffectFile::Teleport(p) => Effect::Teleport(*p),
        EffectFile::Karma(v) => {
            karma(problems, *v);
            Effect::Karma(*v)
        }
        EffectFile::LearnTechnique(name) => match names.techniques.get(name) {
            Some(t) => Effect::LearnTechnique(*t),
            None => {
                problems.push(format!("technique `{name}` not found"));
                return None;
            }
        },
    })
}
