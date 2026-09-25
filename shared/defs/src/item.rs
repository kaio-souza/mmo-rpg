//! Items (phase 3: stackable only).
//!
//! ```ron
//! // content/items/glow_herb.ron
//! (kind: Quest, max_stack: 20, weight: 50, binding: Character, rarity: Common)
//! // content/items/travel_bread.ron
//! (kind: Consumable, max_stack: 10, use: Some((hp: 30, cooldown_ticks: 300)))
//! ```
//!
//! Name and description are always `item.<id>.name` and `item.<id>.desc`.

use crate::npc::valid_id;
use base::{Attribute, ItemKind, ProficiencyId, Rarity, TextId};
use serde::{Deserialize, Serialize};

/// Largest pile of one item in one backpack slot.
pub const MAX_STACK: u16 = 9999;

/// Who may hold the item after it is picked up
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum Binding {
    #[default]
    Free,
    Character,
    Account,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDef {
    /// File stem, `[a-z0-9_]`; what saves store.
    pub id: String,
    pub kind: ItemKind,
    pub max_stack: u16,
    /// Grams.
    pub weight: u32,
    pub binding: Binding,
    pub rarity: Rarity,
    /// `item.<id>.name`.
    pub name: TextId,
    /// `item.<id>.desc`.
    pub description: TextId,
    /// Kinds `Weapon` and `Armor` only.
    pub equip: Option<Equip>,
    /// Bonuses rolled when an instance is created, hidden until `Appraise`.
    pub hidden: Vec<HiddenRoll>,
    /// Kind `Consumable` only: what using one does (phase 5).
    pub use_effect: Option<UseEffect>,
    /// What the item is worth, in copper (phase 7): an NPC sells at this
    /// and buys at a share of it. 0:
    /// no NPC trades it.
    pub value: u64,
    /// Equipment only (phase 7): blows it takes before breaking. 0: never
    /// wears out.
    pub durability: u32,
    /// Icon name in `assets/icons/` (empty: generic per kind).
    pub icon: String,
    /// Equipment only: the powers a piece of Unique rarity or above may
    /// roll, one of them.
    pub powers: Vec<base::ItemPower>,
    /// Phase 11: light around the wearer while equipped.
    pub light: Option<ItemLight>,
    /// Phase 11: durability lost per minute equipped (a torch
    /// burns down); 0: none.
    pub burn: u32,
}

/// A power as written in the item file: a technique by id, or an
/// attribute bonus.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub(crate) enum PowerFile {
    Technique(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::TechniqueRef"))] String),
    Attribute(Attribute, u16),
}

/// Using one item of the stack: restores HP, vigor and MP, then the item
/// rests for `cooldown_ticks` (all items of this kind). With
/// `revive_permille`, the item is used on a fallen player nearby instead
/// they stand up with that share of their maximum HP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UseEffect {
    #[serde(default)]
    pub hp: u32,
    #[serde(default)]
    pub vigor: u32,
    #[serde(default)]
    pub cooldown_ticks: u32,
    /// Thousandths of the maximum HP a fallen player stands up with; 0:
    /// not a revival item.
    #[serde(default)]
    pub revive_permille: u16,
    /// MP restored (P02, the mana tonic).
    #[serde(default)]
    pub mp: u32,
}

/// What equipping the item does (phase 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Equip {
    Weapon {
        damage: (u32, u32),
        attack_ticks: u32,
        /// Attribute that adds damage (STR for swords, DEX for daggers...).
        attribute: Attribute,
        proficiency: ProficiencyId,
        range: u16,
        /// Sprite drawn over the body; empty: none.
        visual: String,
        /// Alpha: vigor each blow costs; without it the blow waits twice
        /// as long (heavy weapons tire).
        vigor: u32,
    },
    Armor {
        defense: u32,
        visual: String,
    },
    /// Phase 11: the other hand (a torch).
    Offhand {
        defense: u32,
        visual: String,
    },
}

impl Equip {
    /// Sprite drawn over the body; empty: none.
    pub fn visual(&self) -> &str {
        match self {
            Equip::Weapon { visual, .. } | Equip::Armor { visual, .. } | Equip::Offhand { visual, .. } => {
                visual
            }
        }
    }
}

/// Light an equipped item gives around its wearer (phase 11):
/// only drawn, no rule reads it. `radius` in tiles is how bright it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ItemLight {
    pub radius: u8,
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ColorRef"))]
    pub color: (u8, u8, u8),
}

/// Most tiles an item light reaches.
pub const MAX_LIGHT_RADIUS: u8 = 16;

/// A hidden attribute bonus: `min..=max` rolled on creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct HiddenRoll {
    pub attribute: Attribute,
    pub min: u16,
    pub max: u16,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) enum EquipFile {
    Weapon {
        damage: (u32, u32),
        attack_ticks: u32,
        attribute: Attribute,
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ProficiencyRef"))]
        proficiency: String,
        #[serde(default = "one_u16")]
        range: u16,
        #[serde(default)]
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::SpriteRef"))]
        visual: String,
        #[serde(default)]
        vigor: u32,
    },
    Armor {
        defense: u32,
        #[serde(default)]
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::SpriteRef"))]
        visual: String,
    },
    Offhand {
        #[serde(default)]
        defense: u32,
        #[serde(default)]
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::SpriteRef"))]
        visual: String,
    },
}

fn one_u16() -> u16 {
    1
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ItemFile {
    pub kind: ItemKind,
    #[serde(default = "one")]
    pub max_stack: u16,
    #[serde(default)]
    pub weight: u32,
    #[serde(default)]
    pub binding: Binding,
    #[serde(default)]
    pub rarity: Rarity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equip: Option<EquipFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<HiddenRoll>,
    #[serde(default, rename = "use", skip_serializing_if = "Option::is_none")]
    pub use_effect: Option<UseEffect>,
    #[serde(default)]
    pub value: u64,
    #[serde(default)]
    pub durability: u32,
    #[serde(default)]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::IconRef"))]
    pub icon: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub powers: Vec<PowerFile>,
    /// Light around the wearer while equipped, seen by everyone at night:
    /// radius in tiles (how bright, 1 to 16) and color. Any equipment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<ItemLight>,
    /// Durability lost each minute it is equipped (a torch burns down; at
    /// zero it is gone). 0: never. Needs durability.
    #[serde(default)]
    pub burn: u32,
}

fn one() -> u16 {
    1
}

pub(crate) fn parse_item(id: &str, text: &str) -> Result<ItemFile, String> {
    if !valid_id(id) {
        return Err(format!("item id `{id}` must be [a-z0-9_]"));
    }
    let file: ItemFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if file.max_stack == 0 || file.max_stack > MAX_STACK {
        return Err(format!("max_stack {} must be 1 to {MAX_STACK}", file.max_stack));
    }
    let equipment = matches!(file.kind, ItemKind::Weapon | ItemKind::Armor | ItemKind::Offhand);
    match &file.equip {
        Some(EquipFile::Weapon {
            damage,
            attack_ticks,
            range,
            ..
        }) => {
            if file.kind != ItemKind::Weapon || damage.0 > damage.1 || *attack_ticks == 0 || *range == 0 {
                return Err("Weapon: kind Weapon, damage min <= max, attack_ticks and range >= 1".into());
            }
        }
        Some(EquipFile::Armor { .. }) if file.kind != ItemKind::Armor => {
            return Err("Armor needs kind Armor".into());
        }
        Some(EquipFile::Armor { .. }) => {}
        Some(EquipFile::Offhand { .. }) if file.kind != ItemKind::Offhand => {
            return Err("Offhand needs kind Offhand".into());
        }
        Some(EquipFile::Offhand { .. }) => {}
        None if equipment => return Err("kinds Weapon, Armor and Offhand need `equip`".into()),
        None => {}
    }
    if equipment && file.max_stack != 1 {
        return Err("equipment has its own identity: max_stack must be 1".into());
    }
    if !equipment && !file.hidden.is_empty() {
        return Err("only equipment has hidden attributes".into());
    }
    if !equipment && file.durability != 0 {
        return Err("only equipment has durability".into());
    }
    if !equipment && file.light.is_some() {
        return Err("only equipment gives light".into());
    }
    if file
        .light
        .is_some_and(|l| l.radius == 0 || l.radius > MAX_LIGHT_RADIUS)
    {
        return Err(format!("light: radius is 1 to {MAX_LIGHT_RADIUS} tiles"));
    }
    if file.burn > 0 && file.durability == 0 {
        return Err("burn wears durability: it needs durability".into());
    }
    if !equipment && !file.powers.is_empty() {
        return Err("only equipment has powers".into());
    }
    if file
        .powers
        .iter()
        .any(|p| matches!(p, PowerFile::Attribute(_, 0)))
    {
        return Err("powers: an attribute bonus must be at least 1".into());
    }
    match file.use_effect {
        Some(_) if file.kind != ItemKind::Consumable => return Err("only kind Consumable has `use`".into()),
        Some(u) if u.revive_permille > 1000 => return Err("use: revive_permille is at most 1000".into()),
        Some(u) if u.revive_permille > 0 && (u.hp > 0 || u.vigor > 0 || u.mp > 0) => {
            return Err("use: a revival item restores nothing else (hp, vigor and mp 0)".into());
        }
        Some(u) if u.hp == 0 && u.vigor == 0 && u.mp == 0 && u.revive_permille == 0 => {
            return Err("use: hp, vigor, mp or revive_permille must be at least 1".into());
        }
        _ => {}
    }
    if file.hidden.iter().any(|h| h.min > h.max) {
        return Err("hidden: min <= max".into());
    }
    Ok(file)
}

pub fn item_name_key(id: &str) -> String {
    format!("item.{id}.name")
}

pub fn item_desc_key(id: &str) -> String {
    format!("item.{id}.desc")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_only_on_consumables_and_restoring_something() {
        let ok = parse_item(
            "bread",
            "(kind: Consumable, use: Some((hp: 30, cooldown_ticks: 300)))",
        )
        .unwrap();
        assert_eq!(
            ok.use_effect,
            Some(UseEffect {
                hp: 30,
                vigor: 0,
                cooldown_ticks: 300,
                revive_permille: 0,
                mp: 0,
            })
        );
        let revive = parse_item(
            "draught",
            "(kind: Consumable, use: Some((revive_permille: 300, cooldown_ticks: 600)))",
        )
        .unwrap();
        assert_eq!(revive.use_effect.map(|u| u.revive_permille), Some(300));
        assert!(
            parse_item(
                "x",
                "(kind: Consumable, use: Some((hp: 5, revive_permille: 300)))"
            )
            .is_err()
        );
        assert!(parse_item("x", "(kind: Consumable, use: Some((revive_permille: 1001)))").is_err());
        assert!(parse_item("herb", "(kind: Quest, use: Some((hp: 1)))").is_err());
        assert!(parse_item("water", "(kind: Consumable, use: Some((cooldown_ticks: 5)))").is_err());
        assert_eq!(parse_item("pelt", "(kind: Material)").unwrap().use_effect, None);
    }
}
