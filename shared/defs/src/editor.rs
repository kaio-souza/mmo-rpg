//! What `tools/editor` needs from the content types (feature `editor`,
//!): the list of editable kinds, a JSON schema of each file
//! format, and lossless conversion between the RON file and JSON.
//!
//! The editor never knows a content rule: it converts with the same file
//! types the loader uses and validates by loading the whole catalog
//! ([`crate::from_text`] and [`crate::Catalog::validate`]).
//!
//! Reference fields carry `"x-ref": "<kind>"` in the schema so the editor
//! offers a list of existing ids; text keys carry `"x-ref": "text"`.
//! A fixed-size table names its rows by an enum (`"x-by"`, see
//! [`ByRarity`]), and a field only some values of another field use says
//! so with `"x-when": {"<field>": [values]}` (equipment-only item fields).

use crate::appearance::AppearanceFile;
use crate::craft::{NodeFile, NodeSpawn, RecipeFile};
use crate::economy::{LootTableFile, ShopFile};
use crate::emote::EmoteFile;
use crate::formulas::{CombatFormulas, EconomyFormulas, ProgressionFormulas};
use crate::item::ItemFile;
use crate::npc::{DialogueFile, NpcFile, NpcSpawnFile};
use crate::progress::{
    ClassFile, MonsterFile, ProficiencyFile, SpecialAttributeFile, TechniqueFile, TitleFile,
};
use crate::quest::QuestFile;
use crate::zone::{MapFile, MonsterSpawn, ZoneFile};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// One editable kind of content file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    /// Name used by the editor (`item`, `quest`...).
    pub name: &'static str,
    /// Path under `content/`; `{id}` marks a kind with one file per id.
    pub path: &'static str,
    /// Text keys every id of this kind must have (`{id}` replaced).
    pub texts: &'static [&'static str],
}

impl Kind {
    /// One file per id (`items/<id>.ron`), as opposed to a single file.
    pub fn per_id(&self) -> bool {
        self.path.contains("{id}")
    }

    /// Path of the file of `id` (the single path when not per id).
    pub fn file(&self, id: &str) -> String {
        self.path.replace("{id}", id)
    }

    /// The id of `path` when it belongs to this kind.
    pub fn id_of(&self, path: &str) -> Option<String> {
        let (pre, post) = self.path.split_once("{id}")?;
        let id = path.strip_prefix(pre)?.strip_suffix(post)?;
        (!id.is_empty() && !id.contains('/')).then(|| id.to_owned())
    }

    /// Text keys of `id`.
    pub fn text_keys(&self, id: &str) -> Vec<String> {
        self.texts.iter().map(|t| t.replace("{id}", id)).collect()
    }
}

/// Game ticks per second, so the editor can show a `*_ticks` field in
/// seconds without keeping its own copy of the clock.
pub const TICKS_PER_SECOND: u64 = base::TICKS_PER_SECOND;

/// Every kind the editor shows. A new content type is only done with a
/// line here: the `every_content_file_has_a_kind` test fails
/// otherwise.
pub const KINDS: &[Kind] = &[
    Kind {
        name: "npc",
        path: "npcs/{id}.ron",
        texts: &["npc.{id}.name"],
    },
    Kind {
        name: "dialogue",
        path: "npcs/dialogues/{id}.ron",
        texts: &[],
    },
    Kind {
        name: "quest",
        path: "quests/{id}.ron",
        texts: &["quest.{id}.name", "quest.{id}.desc"],
    },
    Kind {
        name: "item",
        path: "items/{id}.ron",
        texts: &["item.{id}.name", "item.{id}.desc"],
    },
    Kind {
        name: "monster",
        path: "monsters/{id}.ron",
        texts: &["monster.{id}.name"],
    },
    Kind {
        name: "technique",
        path: "techniques/{id}.ron",
        texts: &["technique.{id}.name", "technique.{id}.desc"],
    },
    Kind {
        name: "proficiency",
        path: "proficiencies/{id}.ron",
        texts: &["proficiency.{id}.name"],
    },
    Kind {
        name: "class",
        path: "classes/{id}.ron",
        texts: &["class.{id}.name", "class.{id}.desc"],
    },
    Kind {
        name: "title",
        path: "titles/{id}.ron",
        texts: &["title.{id}.name", "title.{id}.desc"],
    },
    Kind {
        name: "special_attribute",
        path: "special_attributes/{id}.ron",
        texts: &["special.{id}.name", "special.{id}.desc"],
    },
    Kind {
        name: "loot_table",
        path: "loot/{id}.ron",
        texts: &[],
    },
    Kind {
        name: "shop",
        path: "shops/{id}.ron",
        texts: &[],
    },
    Kind {
        name: "recipe",
        path: "recipes/{id}.ron",
        texts: &["recipe.{id}.name", "recipe.{id}.desc"],
    },
    Kind {
        name: "gather_node",
        path: "nodes/{id}.ron",
        texts: &["node.{id}.name"],
    },
    Kind {
        name: "emote",
        path: "emotes/{id}.ron",
        texts: &["emote.{id}.name"],
    },
    Kind {
        name: "dungeon",
        path: "dungeons/{id}.ron",
        texts: &["dungeon.{id}.name"],
    },
    Kind {
        name: "ranking",
        path: "rankings/{id}.ron",
        texts: &["ranking.{id}.name"],
    },
    Kind {
        name: "goal",
        path: "goals/{id}.ron",
        texts: &["goal.{id}.name", "goal.{id}.desc"],
    },
    Kind {
        name: "zone",
        path: "zones/{id}/zone.ron",
        texts: &[],
    },
    Kind {
        name: "map",
        path: "zones/{id}/map.ron",
        texts: &[],
    },
    Kind {
        name: "zone_npcs",
        path: "zones/{id}/npcs.ron",
        texts: &[],
    },
    Kind {
        name: "zone_monsters",
        path: "zones/{id}/monsters.ron",
        texts: &[],
    },
    Kind {
        name: "zone_nodes",
        path: "zones/{id}/nodes.ron",
        texts: &[],
    },
    Kind {
        name: "appearance",
        path: "appearance/appearance.ron",
        texts: &[],
    },
    Kind {
        name: "counters",
        path: "counters.ron",
        texts: &[],
    },
    Kind {
        name: "moderation_reasons",
        path: "moderation/reasons.ron",
        texts: &[],
    },
    Kind {
        name: "progression_formulas",
        path: "progression/formulas.ron",
        texts: &[],
    },
    Kind {
        name: "combat_formulas",
        path: "combat/formulas.ron",
        texts: &[],
    },
    Kind {
        name: "economy_formulas",
        path: "economy/formulas.ron",
        texts: &[],
    },
];

/// Files the editor shows as raw text (no form): the text tables, which
/// have their own screen.
pub const RAW_PATHS: &[&str] = &["text/"];

pub fn kind(name: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.name == name)
}

/// The kind and id of a path under `content/`.
pub fn kind_of(path: &str) -> Option<(&'static Kind, String)> {
    KINDS.iter().find_map(|k| {
        if k.per_id() {
            k.id_of(path).map(|id| (k, id))
        } else {
            (k.path == path).then(|| (k, String::new()))
        }
    })
}

fn schema_of<T: JsonSchema>() -> serde_json::Value {
    let s = schemars::generate::SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<T>();
    s.to_value()
}

fn json_of<T: DeserializeOwned + Serialize>(text: &str) -> Result<serde_json::Value, String> {
    let v: T = ron::from_str(text).map_err(|e| e.to_string())?;
    serde_json::to_value(&v).map_err(|e| e.to_string())
}

fn ron_of<T: DeserializeOwned + Serialize>(v: serde_json::Value, depth: usize) -> Result<String, String> {
    let v: T = serde_json::from_value(v).map_err(|e| e.to_string())?;
    let cfg = ron::ser::PrettyConfig::new()
        .indentor("    ".to_owned())
        .depth_limit(depth)
        .separate_tuple_members(false);
    let mut out = ron::ser::to_string_pretty(&v, cfg).map_err(|e| e.to_string())?;
    out.push('\n');
    Ok(out)
}

macro_rules! by_kind {
    ($name:expr, $f:ident $(, $arg:expr)*) => {
        match $name {
            "npc" => Some($f::<NpcFile>($($arg),*)),
            "dialogue" => Some($f::<DialogueFile>($($arg),*)),
            "quest" => Some($f::<QuestFile>($($arg),*)),
            "item" => Some($f::<ItemFile>($($arg),*)),
            "monster" => Some($f::<MonsterFile>($($arg),*)),
            "technique" => Some($f::<TechniqueFile>($($arg),*)),
            "proficiency" => Some($f::<ProficiencyFile>($($arg),*)),
            "class" => Some($f::<ClassFile>($($arg),*)),
            "title" => Some($f::<TitleFile>($($arg),*)),
            "special_attribute" => Some($f::<SpecialAttributeFile>($($arg),*)),
            "zone" => Some($f::<ZoneFile>($($arg),*)),
            "map" => Some($f::<MapFile>($($arg),*)),
            "zone_npcs" => Some($f::<Vec<NpcSpawnFile>>($($arg),*)),
            "zone_monsters" => Some($f::<Vec<MonsterSpawn>>($($arg),*)),
            "zone_nodes" => Some($f::<Vec<NodeSpawn>>($($arg),*)),
            "loot_table" => Some($f::<LootTableFile>($($arg),*)),
            "shop" => Some($f::<ShopFile>($($arg),*)),
            "recipe" => Some($f::<RecipeFile>($($arg),*)),
            "gather_node" => Some($f::<NodeFile>($($arg),*)),
            "emote" => Some($f::<EmoteFile>($($arg),*)),
            "ranking" => Some($f::<crate::rank::RankingFile>($($arg),*)),
            "goal" => Some($f::<crate::goal::GoalFile>($($arg),*)),
            "dungeon" => Some($f::<crate::dungeon::DungeonFile>($($arg),*)),
            "appearance" => Some($f::<AppearanceFile>($($arg),*)),
            "progression_formulas" => Some($f::<ProgressionFormulas>($($arg),*)),
            "combat_formulas" => Some($f::<CombatFormulas>($($arg),*)),
            "economy_formulas" => Some($f::<EconomyFormulas>($($arg),*)),
            "counters" => Some($f::<CounterList>($($arg),*)),
            "moderation_reasons" => Some($f::<ReasonList>($($arg),*)),
            _ => None,
        }
    };
}

/// JSON schema (draft 2020-12) of a kind's file (every kind has a form).
pub fn schema(kind: &str) -> Option<serde_json::Value> {
    by_kind!(kind, schema_of)
}

/// Reads a content file into JSON with the loader's own types.
pub fn to_json(kind: &str, text: &str) -> Result<serde_json::Value, String> {
    by_kind!(kind, json_of, text).unwrap_or_else(|| Err(format!("`{kind}` has no form")))
}

/// Writes JSON from the editor back as the RON file, in a fixed format.
pub fn to_ron(kind: &str, value: serde_json::Value) -> Result<String, String> {
    // Lists of spawns and map legends stay one entry per line, as written by hand.
    let depth = match kind {
        "zone_npcs" | "zone_monsters" | "zone_nodes" | "zone" => 1,
        "map" => 2,
        _ => usize::MAX,
    };
    by_kind!(kind, ron_of, value, depth).unwrap_or_else(|| Err(format!("`{kind}` has no form")))
}

/// Leading `//` lines of a file, kept when the editor rewrites it.
pub fn header(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("//") {
            out.push_str(line);
            out.push('\n');
        } else {
            break;
        }
    }
    out
}

/// `counters.ron`: the families on, each at most once, from the families
/// the engine knows (`COUNTER_FAMILIES`), so the page shows switches.
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct CounterList(Vec<String>);

impl JsonSchema for CounterList {
    fn schema_name() -> Cow<'static, str> {
        "CounterList".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "array",
            "uniqueItems": true,
            "items": { "type": "string", "enum": crate::progress::COUNTER_FAMILIES },
        })
    }
}

/// `moderation/reasons.ron`: the text keys a moderator may pick as reason.
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct ReasonList(Vec<String>);

impl JsonSchema for ReasonList {
    fn schema_name() -> Cow<'static, str> {
        "ReasonList".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "array",
            "uniqueItems": true,
            "items": { "type": "string", "x-ref": "text", "x-key-prefix": "moderation.reason." },
        })
    }
}

macro_rules! refs {
    ($($name:ident => $kind:literal),* $(,)?) => {$(
        #[doc = concat!("Schema of a string that names a `", $kind, "`.")]
        pub struct $name;
        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }
            fn inline_schema() -> bool {
                true
            }
            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "string", "x-ref": $kind })
            }
        }
    )*};
}

/// An RGB color `(r, g, b)` of 0..=255: the form shows a color picker.
pub struct ColorRef;

impl JsonSchema for ColorRef {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ColorRef".into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "array", "x-color": true, "minItems": 3, "maxItems": 3,
            "prefixItems": [
                { "type": "integer", "minimum": 0, "maximum": 255 },
                { "type": "integer", "minimum": 0, "maximum": 255 },
                { "type": "integer", "minimum": 0, "maximum": 255 }
            ]
        })
    }
}

/// Six numbers, one per `Rarity` (Common to Mythic): the form names each
/// row by its rarity and the list keeps its size.
pub struct ByRarity;

impl JsonSchema for ByRarity {
    fn schema_name() -> Cow<'static, str> {
        "ByRarity".into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let by = generator.subschema_for::<base::Rarity>();
        json_schema!({
            "type": "array", "minItems": 6, "maxItems": 6, "x-by": by,
            "items": { "type": "integer", "format": "uint32", "minimum": 0 }
        })
    }
}

refs! {
    ItemRef => "item",
    NpcRef => "npc",
    QuestRef => "quest",
    DialogueRef => "dialogue",
    ProficiencyRef => "proficiency",
    ClassRef => "class",
    TechniqueRef => "technique",
    TitleRef => "title",
    SpecialAttributeRef => "special_attribute",
    MonsterRef => "monster",
    NodeRef => "node",
    LootTableRef => "loot_table",
    ShopRef => "shop",
    RecipeRef => "recipe",
    GatherNodeRef => "gather_node",
    TextKey => "text",
    SpriteRef => "sprite",
    IconRef => "icon",
    CounterFamilyRef => "counter_family",
    CounterKeyRef => "counter_key",
    ZoneRef => "zone",
    DungeonRef => "dungeon",
    GoalRef => "goal",
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn paths_map_to_kinds() {
        assert_eq!(
            kind_of("items/glow_herb.ron").map(|(k, id)| (k.name, id)),
            Some(("item", "glow_herb".into()))
        );
        assert_eq!(
            kind_of("npcs/dialogues/guide.ron").map(|(k, _)| k.name),
            Some("dialogue")
        );
        assert_eq!(kind_of("npcs/guide.ron").map(|(k, _)| k.name), Some("npc"));
        assert_eq!(
            kind_of("zones/dev_field/npcs.ron").map(|(k, id)| (k.name, id)),
            Some(("zone_npcs", "dev_field".into()))
        );
        assert_eq!(
            kind_of("combat/formulas.ron").map(|(k, _)| k.name),
            Some("combat_formulas")
        );
        assert!(kind_of("items/a/b.ron").is_none());
    }

    #[test]
    fn every_kind_with_a_form_has_a_schema() {
        for k in KINDS {
            assert!(schema(k.name).is_some(), "{}", k.name);
        }
        let s = schema("technique").unwrap().to_string();
        assert!(s.contains("\"x-ref\":\"proficiency\""), "{s}");
    }

    #[test]
    fn round_trip_keeps_the_meaning() {
        let text = "// header\n(kind: Consumable, max_stack: 10, use: Some((hp: 30, cooldown_ticks: 300)))";
        let json = to_json("item", text).unwrap();
        let ron = to_ron("item", json.clone()).unwrap();
        assert_eq!(to_json("item", &ron).unwrap(), json);
        assert_eq!(header(text), "// header\n");
        let dialogue = r#"(start: "a", nodes: { "a": (text: "k", options: [(text: "o", when: Coins(5))]) })"#;
        let json = to_json("dialogue", dialogue).unwrap();
        let ron = to_ron("dialogue", json.clone()).unwrap();
        assert!(ron.contains("when: Coins(5)"), "{ron}");
        assert_eq!(to_json("dialogue", &ron).unwrap(), json);
    }
}
