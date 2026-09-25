//! NPCs and dialogue.
//!
//! ```ron
//! // content/npcs/elder.ron
//! (sprite: "elder", dialogue: "elder")
//! // content/npcs/dialogues/elder.ron
//! (
//!     start: "greeting",
//!     nodes: {
//!         "greeting": (
//!             text: "npc.elder.greeting",
//!             options: [(text: "npc.elder.option.bye")],
//!         ),
//!     },
//! )
//! ```
//!
//! No game text lives here: nodes and options only name text keys. The name
//! of an NPC is always `npc.<id>.name` ([i18n]).

use crate::quest::Names;
use crate::rules::{self, Condition, ConditionFile, Effect, EffectFile, MAX_EFFECTS};
use base::{DialogueId, ShopId, TextId, TilePos};
use i18n::KeyTable;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// More options than keys the terminal client can number.
pub const MAX_OPTIONS: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcDef {
    /// File stem, `[a-z0-9_]`.
    pub id: String,
    /// Sprite name in `assets/` (empty: test figure).
    pub sprite: String,
    /// Dialogue opened by `Talk`; `None` for a silent NPC.
    pub dialogue: Option<DialogueId>,
    /// Shop this NPC keeps (phase 7); `None`: sells nothing.
    pub shop: Option<ShopId>,
    /// `npc.<id>.name`.
    pub name: TextId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueDef {
    /// File stem, `[a-z0-9_]`.
    pub id: String,
    /// Index of the first node in `nodes`.
    pub start: u16,
    /// Sorted by node name; `start` and `next` point here.
    pub nodes: Vec<DialogueNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueNode {
    /// Name in the file, kept for diagnostics.
    pub name: String,
    pub text: TextId,
    pub options: Vec<DialogueOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueOption {
    pub text: TextId,
    /// `None`: the option closes the dialogue.
    pub next: Option<u16>,
    /// Hidden while this does not hold.
    pub when: Option<Condition>,
    /// Run in order when picked, all or nothing.
    pub effects: Vec<Effect>,
}

/// One entry of `content/zones/<zone>/npcs.ron`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcSpawn {
    /// NPC id (`content/npcs/<npc>.ron`).
    pub npc: String,
    pub pos: TilePos,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct NpcFile {
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::SpriteRef"))]
    pub sprite: String,
    /// Empty: no dialogue.
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::DialogueRef"))]
    pub dialogue: String,
    /// Empty: no shop (phase 7).
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ShopRef"))]
    pub shop: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct DialogueFile {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::NodeRef"))]
    start: String,
    nodes: BTreeMap<String, NodeFile>,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeFile {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::TextKey"))]
    text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    options: Vec<OptionFile>,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct OptionFile {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::TextKey"))]
    text: String,
    /// Empty: closes the dialogue.
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::NodeRef"))]
    next: String,
    /// Written without `Some(...)`: `when: Coins(5)`.
    #[serde(
        default,
        deserialize_with = "some",
        serialize_with = "some_ser",
        skip_serializing_if = "Option::is_none"
    )]
    when: Option<ConditionFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    effects: Vec<EffectFile>,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct NpcSpawnFile {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::NpcRef"))]
    pub npc: String,
    pub pos: TilePos,
}

#[allow(clippy::ref_option)] // serde passes the field by reference
fn some_ser<S: serde::Serializer>(v: &Option<ConditionFile>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(c) => c.serialize(s),
        None => s.serialize_none(),
    }
}

fn some<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<ConditionFile>, D::Error> {
    ConditionFile::deserialize(d).map(Some)
}

/// A content id is a file stem, never free text.
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

pub(crate) fn parse_npc(id: &str, text: &str) -> Result<NpcFile, String> {
    if !valid_id(id) {
        return Err(format!("npc id `{id}` must be [a-z0-9_]"));
    }
    ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))
}

pub(crate) fn parse_spawns(text: &str) -> Result<Vec<NpcSpawnFile>, String> {
    ron::from_str(text).map_err(|e| format!("npcs.ron: {e}"))
}

/// Resolves every text key and every node name; on success the tree only
/// contains indices, ready for the simulation.
pub(crate) fn parse_dialogue(
    id: &str,
    text: &str,
    keys: &KeyTable,
    names: &Names<'_>,
) -> Result<DialogueDef, Vec<String>> {
    if !valid_id(id) {
        return Err(vec![format!("id `{id}` must be [a-z0-9_]")]);
    }
    let file: DialogueFile = ron::from_str(text).map_err(|e| vec![format!("dialogues/{id}.ron: {e}")])?;
    let mut problems = Vec::new();
    let mut indices: BTreeMap<&str, u16> = BTreeMap::new();
    if file.nodes.len() > usize::from(u16::MAX) {
        return Err(vec!["too many nodes".to_owned()]);
    }
    for name in file.nodes.keys() {
        if !valid_id(name) {
            problems.push(format!("node `{name}` must be [a-z0-9_]"));
        }
    }
    for (i, name) in file.nodes.keys().enumerate() {
        if let Ok(i) = u16::try_from(i) {
            indices.insert(name.as_str(), i);
        }
    }
    let start = match indices.get(file.start.as_str()) {
        Some(i) => *i,
        None => {
            problems.push(format!("start `{}` is not a node", file.start));
            0
        }
    };
    let text_id = |problems: &mut Vec<String>, where_: &str, key: &str| -> Option<TextId> {
        match keys.id(key) {
            Some(text) => Some(text),
            None => {
                problems.push(format!("{where_}: no text key `{key}`"));
                None
            }
        }
    };
    let mut nodes = Vec::with_capacity(file.nodes.len());
    for (name, node) in &file.nodes {
        if node.options.len() > MAX_OPTIONS {
            problems.push(format!(
                "node `{name}` has {} options, at most {MAX_OPTIONS}",
                node.options.len()
            ));
        }
        let Some(text) = text_id(&mut problems, &format!("node `{name}`"), &node.text) else {
            continue;
        };
        let mut options = Vec::with_capacity(node.options.len());
        for (i, option) in node.options.iter().enumerate() {
            let Some(text) = text_id(&mut problems, &format!("node `{name}` option {i}"), &option.text)
            else {
                continue;
            };
            let next = if option.next.is_empty() {
                None
            } else {
                match indices.get(option.next.as_str()) {
                    Some(index) => Some(*index),
                    None => {
                        problems.push(format!(
                            "node `{name}` option {i}: next `{}` is not a node",
                            option.next
                        ));
                        continue;
                    }
                }
            };
            let mut rule_problems = Vec::new();
            let when = option
                .when
                .as_ref()
                .and_then(|c| rules::condition(names, &mut rule_problems, c, 0));
            if option.effects.len() > MAX_EFFECTS {
                rule_problems.push(format!("more than {MAX_EFFECTS} effects"));
            }
            let effects: Vec<Effect> = option
                .effects
                .iter()
                .filter_map(|e| rules::effect(names, &mut rule_problems, e))
                .collect();
            if !rule_problems.is_empty() {
                problems.extend(
                    rule_problems
                        .into_iter()
                        .map(|p| format!("node `{name}` option {i}: {p}")),
                );
                continue;
            }
            options.push(DialogueOption {
                text,
                next,
                when,
                effects,
            });
        }
        nodes.push(DialogueNode {
            name: name.clone(),
            text,
            options,
        });
    }
    if problems.is_empty() {
        Ok(DialogueDef {
            id: id.to_owned(),
            start,
            nodes,
        })
    } else {
        Err(problems)
    }
}

/// Text key of an NPC name; the only key derived from the id.
pub fn name_key(id: &str) -> String {
    format!("npc.{id}.name")
}
