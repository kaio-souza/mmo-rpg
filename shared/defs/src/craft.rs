//! Gathering and crafting content (phase 8): recipes and resource nodes.
//!
//! ```ron
//! // content/recipes/iron_sword.ron
//! (proficiency: "smithing", min_points: 0,
//!  materials: [(item: "iron_ingot", qty: 2)], result: "iron_sword", qty: 1, xp: 40)
//! // content/nodes/copper_vein.ron
//! (sprite: "copper_vein", proficiency: "mining", cooldown_ticks: 600, loot: "copper_vein")
//! // content/zones/dev_field/nodes.ron
//! [(node: "copper_vein", pos: (x: 10, y: 10))]
//! ```
//!
//! A node is emptied **per player** and comes back only for whoever used
//! it. What it yields is a
//! loot table, so the same table serves a monster and a node.

use crate::npc::valid_id;
use base::{ItemId, LootTableId, ProficiencyId, TextId, TilePos};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most lines of materials one recipe may ask for.
pub const MAX_MATERIALS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Material {
    pub item: ItemId,
    pub qty: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeDef {
    pub id: String,
    /// `recipe.<id>.name`.
    pub name: TextId,
    /// `recipe.<id>.desc`.
    pub description: TextId,
    pub proficiency: ProficiencyId,
    /// Proficiency points needed to craft it.
    pub min_points: u8,
    pub materials: Vec<Material>,
    pub result: ItemId,
    pub qty: u32,
    /// Base XP of one craft; the class multiplies it
    pub xp: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterialFile {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
    pub item: String,
    #[serde(default = "one")]
    pub qty: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct RecipeFile {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ProficiencyRef"))]
    pub proficiency: String,
    #[serde(default)]
    pub min_points: u8,
    pub materials: Vec<MaterialFile>,
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
    pub result: String,
    #[serde(default = "one")]
    pub qty: u32,
    #[serde(default)]
    pub xp: u64,
}

fn one() -> u32 {
    1
}

pub(crate) fn parse_recipe(id: &str, text: &str) -> Result<RecipeFile, String> {
    if !valid_id(id) {
        return Err(format!("recipe id `{id}` must be [a-z0-9_]"));
    }
    let f: RecipeFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if f.materials.is_empty() || f.materials.len() > MAX_MATERIALS {
        return Err(format!("materials: 1 to {MAX_MATERIALS} lines"));
    }
    if f.qty == 0 {
        return Err("qty must be >= 1".into());
    }
    let mut seen: Vec<&str> = Vec::new();
    for m in &f.materials {
        if m.qty == 0 {
            return Err(format!("material `{}`: qty >= 1", m.item));
        }
        if seen.contains(&m.item.as_str()) {
            return Err(format!("materials: item `{}` twice", m.item));
        }
        seen.push(&m.item);
    }
    Ok(f)
}

pub fn recipe_name_key(id: &str) -> String {
    format!("recipe.{id}.name")
}

pub fn recipe_desc_key(id: &str) -> String {
    format!("recipe.{id}.desc")
}

/// A resource node: gathered with a proficiency, empties per player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDef {
    pub id: String,
    /// `node.<id>.name`.
    pub name: TextId,
    pub sprite: String,
    pub proficiency: ProficiencyId,
    pub min_points: u8,
    /// Ticks until it comes back for whoever gathered it.
    pub cooldown_ticks: u32,
    /// What one gathering yields.
    pub loot: LootTableId,
    /// Base XP of one gathering.
    pub xp: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeFile {
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::SpriteRef"))]
    pub sprite: String,
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ProficiencyRef"))]
    pub proficiency: String,
    #[serde(default)]
    pub min_points: u8,
    #[serde(default)]
    pub cooldown_ticks: u32,
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::LootTableRef"))]
    pub loot: String,
    #[serde(default)]
    pub xp: u64,
}

pub(crate) fn parse_node(id: &str, text: &str) -> Result<NodeFile, String> {
    if !valid_id(id) {
        return Err(format!("node id `{id}` must be [a-z0-9_]"));
    }
    ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))
}

pub fn node_name_key(id: &str) -> String {
    format!("node.{id}.name")
}

/// One node placed in a zone (`zones/<zone>/nodes.ron`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct NodeSpawn {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::GatherNodeRef"))]
    pub node: String,
    pub pos: TilePos,
}

pub(crate) fn parse_nodes(text: &str) -> Result<Vec<NodeSpawn>, String> {
    ron::from_str(text).map_err(|e| format!("nodes.ron: {e}"))
}

pub(crate) fn resolve_materials(
    file: Vec<MaterialFile>,
    items: &BTreeMap<String, ItemId>,
) -> Result<Vec<Material>, String> {
    file.into_iter()
        .map(|m| match items.get(&m.item) {
            Some(&item) => Ok(Material { item, qty: m.qty }),
            None => Err(format!("material `{}` not found", m.item)),
        })
        .collect()
}

/// Recipes, checked (`content/recipes/<id>.ron`).
pub(crate) fn validate_recipes(
    files: &BTreeMap<String, String>,
    keys: &i18n::KeyTable,
    items: &BTreeMap<String, ItemId>,
    profs: &BTreeMap<String, ProficiencyId>,
    mut err: impl FnMut(String, String),
) -> Vec<RecipeDef> {
    let mut out = Vec::with_capacity(files.len());
    for (id, text) in files {
        let path = format!("recipes/{id}.ron");
        let f = match parse_recipe(id, text) {
            Ok(f) => f,
            Err(m) => {
                err(path, m);
                continue;
            }
        };
        let Some(&proficiency) = profs.get(&f.proficiency) else {
            err(path, format!("proficiency `{}` not found", f.proficiency));
            continue;
        };
        let Some(&result) = items.get(&f.result) else {
            err(path, format!("result item `{}` not found", f.result));
            continue;
        };
        let materials = match resolve_materials(f.materials, items) {
            Ok(m) => m,
            Err(m) => {
                err(path, m);
                continue;
            }
        };
        if materials.iter().any(|m| m.item == result) {
            err(
                path.clone(),
                "the result cannot be one of its own materials".into(),
            );
        }
        let mut text_key = |k: String| {
            keys.id(&k).unwrap_or_else(|| {
                err(path.clone(), format!("no text key `{k}`"));
                TextId::new(0)
            })
        };
        out.push(RecipeDef {
            id: id.clone(),
            name: text_key(recipe_name_key(id)),
            description: text_key(recipe_desc_key(id)),
            proficiency,
            min_points: f.min_points,
            materials,
            result,
            qty: f.qty,
            xp: f.xp,
        });
    }
    out
}

/// Resource nodes, checked (`content/nodes/<id>.ron`).
pub(crate) fn validate_nodes(
    files: &BTreeMap<String, String>,
    keys: &i18n::KeyTable,
    profs: &BTreeMap<String, ProficiencyId>,
    tables: &BTreeMap<String, LootTableId>,
    mut err: impl FnMut(String, String),
) -> Vec<NodeDef> {
    let mut out = Vec::with_capacity(files.len());
    for (id, text) in files {
        let path = format!("nodes/{id}.ron");
        let f = match parse_node(id, text) {
            Ok(f) => f,
            Err(m) => {
                err(path, m);
                continue;
            }
        };
        let Some(&proficiency) = profs.get(&f.proficiency) else {
            err(path, format!("proficiency `{}` not found", f.proficiency));
            continue;
        };
        let Some(&loot) = tables.get(&f.loot) else {
            err(path, format!("loot table `{}` not found", f.loot));
            continue;
        };
        let name = keys.id(&node_name_key(id)).unwrap_or_else(|| {
            err(path.clone(), format!("no text key `{}`", node_name_key(id)));
            TextId::new(0)
        });
        out.push(NodeDef {
            id: id.clone(),
            name,
            sprite: f.sprite,
            proficiency,
            min_points: f.min_points,
            cooldown_ticks: f.cooldown_ticks,
            loot,
            xp: f.xp,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_needs_materials_without_repeats() {
        assert!(parse_recipe("r", "(proficiency: \"smithing\", materials: [], result: \"x\")").is_err());
        assert!(
            parse_recipe(
                "r",
                "(proficiency: \"smithing\", materials: [(item: \"a\"), (item: \"a\")], result: \"x\")"
            )
            .is_err()
        );
        let ok = parse_recipe(
            "r",
            "(proficiency: \"smithing\", materials: [(item: \"a\", qty: 2)], result: \"x\", xp: 40)",
        )
        .unwrap();
        assert_eq!(ok.qty, 1);
        assert_eq!(ok.xp, 40);
    }
}
