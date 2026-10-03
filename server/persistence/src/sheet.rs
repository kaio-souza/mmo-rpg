//! Character sheet on disk: `postcard` bytes plus a version number. Every
//! change to the sheet adds a version and a `From` from the previous one;
//! old rows are migrated when read, never rewritten in place.
//!
//! Content is named by text id (`"glow_herb"`), never by numeric id

use crate::RepoError;
use serde::{Deserialize, Serialize};

/// Version written by this build.
pub const SHEET_VERSION: u16 = 8;

/// The current sheet. Name and position live in columns, not here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SavedSheet {
    /// Look by text id of `content/appearance/` (v4): body, skin,
    /// hairstyle, hair color. Empty ids (older sheets) are the first option.
    pub look: [String; 4],
    /// Copper.
    pub coins: u64,
    /// One entry per backpack slot.
    pub backpack: Vec<Option<SavedStack>>,
    pub quests: Vec<SavedQuest>,
    /// By NPC id; zero is not stored.
    pub affinity: Vec<(String, i32)>,
    /// Phase 4 (v3).
    pub progress: SavedProgress,
    /// Items with identity; each id is also in `item_owners`.
    pub items: Vec<SavedItem>,
    /// Recipes known (phase 8, v5).
    pub recipes: Vec<String>,
    /// Current HP (0: full).
    pub hp: u32,
    /// Unix second before which the character cannot enter.
    pub locked_until: u64,
    /// Alpha (v8): `(shop id, coins paid, unix second the purse opened)`.
    pub shop_sales: Vec<(String, u64, u64)>,
}

/// Level 0: never played in phase 4 (starts at level 1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SavedProgress {
    pub level: u8,
    pub xp: u64,
    pub max_level: u8,
    /// In `Attribute::ALL` order.
    pub attributes: [u16; 6],
    pub free_points: u16,
    pub class: Option<String>,
    pub proficiencies: Vec<(String, u8, u64)>,
    pub techniques: Vec<(String, u8, u64)>,
    pub titles: Vec<String>,
    pub counters: Vec<(String, String, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedItem {
    pub id: u64,
    pub item: String,
    /// `Some(0)` weapon, `Some(1)` armor; `None` in the backpack at `slot`.
    pub equipped: Option<u8>,
    pub slot: u16,
    /// `(attribute index in Attribute::ALL, bonus)`.
    pub hidden: Vec<(u8, u16)>,
    pub appraised: bool,
    /// Phase 7 (v5): durability left and the maximum of this piece.
    /// `(0, 0)` takes the item's own, whole.
    pub durability: (u32, u32),
    /// Phase 8 (v5): who crafted it.
    pub crafter: Option<String>,
    /// Phase 8 (v5): rarity rolled for this piece; `None` takes the
    /// item's own.
    pub rarity: Option<u8>,
    /// Phase 10 (v6): mastery of its crafter, in hundredths of a
    /// percent of extra durability (0: none).
    pub mastery: u16,
    /// Phase 10 (v6): the power it rolled, by text id.
    pub power: Option<SavedPower>,
}

/// A power of a piece as saved. Append only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavedPower {
    /// Technique by text id.
    Technique(String),
    /// `(attribute index in Attribute::ALL, bonus)`.
    Attribute(u8, u16),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedStack {
    pub item: String,
    pub qty: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedQuest {
    pub quest: String,
    pub completed: bool,
    /// Bit `i`: `Talk` objective `i` done.
    pub talked: u8,
    /// Unix second of the last completion (0: never); a repeatable quest
    /// with a cooldown starts again only after it (alpha, sheet v7).
    pub completed_at: u64,
}

/// Version 1 as written to disk. Kept frozen: changes go to a new version.
#[derive(Serialize, Deserialize)]
struct SheetV1 {
    appearance: u16,
}

/// Version 2 (phase 3): coins, backpack, quests, affinity. Frozen.
#[derive(Serialize, Deserialize)]
struct SheetV2 {
    appearance: u16,
    coins: u64,
    backpack: Vec<Option<(String, u32)>>,
    quests: Vec<(String, bool, u8)>,
    affinity: Vec<(String, i32)>,
}

/// Frozen v3 item: `(id, item, equipped, slot, hidden, appraised)`.
type ItemV3 = (u64, String, Option<u8>, u16, Vec<(u8, u16)>, bool);

/// Version 3 (phase 4): progression, items with identity, HP, death lock. Frozen.
#[derive(Serialize, Deserialize)]
struct SheetV3 {
    v2: SheetV2,
    level: u8,
    xp: u64,
    max_level: u8,
    attributes: [u16; 6],
    free_points: u16,
    class: Option<String>,
    proficiencies: Vec<(String, u8, u64)>,
    techniques: Vec<(String, u8, u64)>,
    titles: Vec<String>,
    counters: Vec<(String, String, u64)>,
    items: Vec<ItemV3>,
    hp: u32,
    locked_until: u64,
}

/// Frozen v5 item: durability, crafter and rolled rarity.
type ItemV5 = (
    u64,
    String,
    Option<u8>,
    u16,
    Vec<(u8, u16)>,
    bool,
    (u32, u32),
    Option<String>,
    Option<u8>,
);

/// Version 5 (phases 7 and 8): durability, crafter and rarity of each
/// piece, and the recipes the character knows. Frozen.
#[derive(Serialize, Deserialize)]
struct SheetV5 {
    v4: SheetV4,
    items: Vec<ItemV5>,
    recipes: Vec<String>,
}

/// Version 6 (phase 10): mastery and power of the pieces that
/// have them, by item id. Frozen.
#[derive(Serialize, Deserialize)]
struct SheetV6 {
    v5: SheetV5,
    extras: Vec<(u64, u16, Option<SavedPower>)>,
}

/// Version 7 (alpha): when each quest was last completed, by quest id, for
/// the cooldown of repeatable quests. Frozen.
#[derive(Serialize, Deserialize)]
struct SheetV7 {
    v6: SheetV6,
    completed_at: Vec<(String, u64)>,
}

/// Version 8 (alpha): what each shop paid the character in its
/// current purse period. Frozen.
#[derive(Serialize, Deserialize)]
struct SheetV8 {
    v7: SheetV7,
    shop_sales: Vec<(String, u64, u64)>,
}

impl From<SheetV8> for SavedSheet {
    fn from(v: SheetV8) -> Self {
        SavedSheet {
            shop_sales: v.shop_sales,
            ..SavedSheet::from(v.v7)
        }
    }
}

impl From<SheetV7> for SavedSheet {
    fn from(v: SheetV7) -> Self {
        let mut s = SavedSheet::from(v.v6);
        for (quest, at) in v.completed_at {
            if let Some(q) = s.quests.iter_mut().find(|q| q.quest == quest) {
                q.completed_at = at;
            }
        }
        s
    }
}

impl From<SheetV6> for SavedSheet {
    fn from(v: SheetV6) -> Self {
        let mut s = SavedSheet::from(v.v5);
        for (id, mastery, power) in v.extras {
            if let Some(i) = s.items.iter_mut().find(|i| i.id == id) {
                i.mastery = mastery;
                i.power = power;
            }
        }
        s
    }
}

impl From<SheetV5> for SavedSheet {
    fn from(v: SheetV5) -> Self {
        SavedSheet {
            items: v
                .items
                .into_iter()
                .map(
                    |(id, item, equipped, slot, hidden, appraised, durability, crafter, rarity)| SavedItem {
                        id,
                        item,
                        equipped,
                        slot,
                        hidden,
                        appraised,
                        durability,
                        crafter,
                        rarity,
                        mastery: 0,
                        power: None,
                    },
                )
                .collect(),
            recipes: v.recipes,
            ..SavedSheet::from(v.v4)
        }
    }
}

/// Version 4 (phase 5): the look chosen at creation. The number
/// `appearance` of older versions is dropped: those characters get the
/// default look. Frozen.
#[derive(Serialize, Deserialize)]
struct SheetV4 {
    v3: SheetV3,
    look: [String; 4],
}

impl From<SheetV4> for SavedSheet {
    fn from(v: SheetV4) -> Self {
        SavedSheet {
            look: v.look,
            ..SavedSheet::from(v.v3)
        }
    }
}

impl From<SheetV1> for SheetV2 {
    fn from(v: SheetV1) -> Self {
        SheetV2 {
            appearance: v.appearance,
            coins: 0,
            backpack: Vec::new(),
            quests: Vec::new(),
            affinity: Vec::new(),
        }
    }
}

impl From<SheetV2> for SavedSheet {
    fn from(v: SheetV2) -> Self {
        SavedSheet {
            coins: v.coins,
            backpack: v
                .backpack
                .into_iter()
                .map(|s| s.map(|(item, qty)| SavedStack { item, qty }))
                .collect(),
            quests: v
                .quests
                .into_iter()
                .map(|(quest, completed, talked)| SavedQuest {
                    quest,
                    completed,
                    talked,
                    completed_at: 0,
                })
                .collect(),
            affinity: v.affinity,
            ..SavedSheet::default()
        }
    }
}

impl From<SheetV3> for SavedSheet {
    fn from(v: SheetV3) -> Self {
        SavedSheet {
            progress: SavedProgress {
                level: v.level,
                xp: v.xp,
                max_level: v.max_level,
                attributes: v.attributes,
                free_points: v.free_points,
                class: v.class,
                proficiencies: v.proficiencies,
                techniques: v.techniques,
                titles: v.titles,
                counters: v.counters,
            },
            items: v
                .items
                .into_iter()
                .map(|(id, item, equipped, slot, hidden, appraised)| SavedItem {
                    id,
                    item,
                    equipped,
                    slot,
                    hidden,
                    appraised,
                    // Before phase 7 no piece had durability of its own:
                    // the zone fills it from the item on the way in.
                    durability: (0, 0),
                    crafter: None,
                    rarity: None,
                    mastery: 0,
                    power: None,
                })
                .collect(),
            hp: v.hp,
            locked_until: v.locked_until,
            ..SavedSheet::from(v.v2)
        }
    }
}

pub(crate) fn encode(sheet: &SavedSheet) -> Result<(u16, Vec<u8>), RepoError> {
    let v2 = SheetV2 {
        appearance: 0,
        coins: sheet.coins,
        backpack: sheet
            .backpack
            .iter()
            .map(|s| s.as_ref().map(|s| (s.item.clone(), s.qty)))
            .collect(),
        quests: sheet
            .quests
            .iter()
            .map(|q| (q.quest.clone(), q.completed, q.talked))
            .collect(),
        affinity: sheet.affinity.clone(),
    };
    let p = &sheet.progress;
    let v3 = SheetV3 {
        v2,
        level: p.level,
        xp: p.xp,
        max_level: p.max_level,
        attributes: p.attributes,
        free_points: p.free_points,
        class: p.class.clone(),
        proficiencies: p.proficiencies.clone(),
        techniques: p.techniques.clone(),
        titles: p.titles.clone(),
        counters: p.counters.clone(),
        items: sheet
            .items
            .iter()
            .map(|i| {
                (
                    i.id,
                    i.item.clone(),
                    i.equipped,
                    i.slot,
                    i.hidden.clone(),
                    i.appraised,
                )
            })
            .collect(),
        hp: sheet.hp,
        locked_until: sheet.locked_until,
    };
    let v4 = SheetV4 {
        v3,
        look: sheet.look.clone(),
    };
    let v5 = SheetV5 {
        v4,
        items: sheet
            .items
            .iter()
            .map(|i| {
                (
                    i.id,
                    i.item.clone(),
                    i.equipped,
                    i.slot,
                    i.hidden.clone(),
                    i.appraised,
                    i.durability,
                    i.crafter.clone(),
                    i.rarity,
                )
            })
            .collect(),
        recipes: sheet.recipes.clone(),
    };
    let v6 = SheetV6 {
        v5,
        extras: sheet
            .items
            .iter()
            .filter(|i| i.mastery > 0 || i.power.is_some())
            .map(|i| (i.id, i.mastery, i.power.clone()))
            .collect(),
    };
    let v7 = SheetV7 {
        v6,
        completed_at: sheet
            .quests
            .iter()
            .filter(|q| q.completed_at > 0)
            .map(|q| (q.quest.clone(), q.completed_at))
            .collect(),
    };
    let v8 = SheetV8 {
        v7,
        shop_sales: sheet.shop_sales.clone(),
    };
    postcard::to_stdvec(&v8)
        .map(|b| (SHEET_VERSION, b))
        .map_err(|e| RepoError::Corrupt(e.to_string()))
}

pub(crate) fn decode(version: u16, bytes: &[u8]) -> Result<SavedSheet, RepoError> {
    match version {
        1 => postcard::from_bytes::<SheetV1>(bytes)
            .map(|v1| SavedSheet::from(SheetV2::from(v1)))
            .map_err(|e| RepoError::Corrupt(format!("sheet v1: {e}"))),
        2 => postcard::from_bytes::<SheetV2>(bytes)
            .map(SavedSheet::from)
            .map_err(|e| RepoError::Corrupt(format!("sheet v2: {e}"))),
        3 => postcard::from_bytes::<SheetV3>(bytes)
            .map(SavedSheet::from)
            .map_err(|e| RepoError::Corrupt(format!("sheet v3: {e}"))),
        4 => postcard::from_bytes::<SheetV4>(bytes)
            .map(SavedSheet::from)
            .map_err(|e| RepoError::Corrupt(format!("sheet v4: {e}"))),
        5 => postcard::from_bytes::<SheetV5>(bytes)
            .map(SavedSheet::from)
            .map_err(|e| RepoError::Corrupt(format!("sheet v5: {e}"))),
        6 => postcard::from_bytes::<SheetV6>(bytes)
            .map(SavedSheet::from)
            .map_err(|e| RepoError::Corrupt(format!("sheet v6: {e}"))),
        7 => postcard::from_bytes::<SheetV7>(bytes)
            .map(SavedSheet::from)
            .map_err(|e| RepoError::Corrupt(format!("sheet v7: {e}"))),
        8 => postcard::from_bytes::<SheetV8>(bytes)
            .map(SavedSheet::from)
            .map_err(|e| RepoError::Corrupt(format!("sheet v8: {e}"))),
        v => Err(RepoError::Corrupt(format!("unknown sheet version {v}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_rows_read_as_v2() {
        let old = postcard::to_stdvec(&SheetV1 { appearance: 4 }).unwrap();
        let s = decode(1, &old).unwrap();
        assert_eq!(s, SavedSheet::default());
    }

    #[test]
    fn v2_roundtrip() {
        let s = SavedSheet {
            look: ["body_a".into(), "skin_3".into(), "short".into(), "black".into()],
            coins: 150,
            backpack: vec![
                None,
                Some(SavedStack {
                    item: "glow_herb".into(),
                    qty: 3,
                }),
            ],
            quests: vec![SavedQuest {
                quest: "first_steps".into(),
                completed: true,
                talked: 1,
                completed_at: 1_700_000_000,
            }],
            affinity: vec![("herbalist".into(), 5)],
            progress: SavedProgress {
                level: 7,
                xp: 30,
                max_level: 8,
                attributes: [10, 9, 5, 5, 5, 5],
                free_points: 2,
                class: Some("warrior".into()),
                proficiencies: vec![("sword".into(), 12, 3)],
                techniques: vec![("double_cut".into(), 2, 5)],
                titles: vec!["wolf_bane".into()],
                counters: vec![("kills".into(), "wolf".into(), 9)],
            },
            items: vec![SavedItem {
                id: 77,
                item: "sword".into(),
                equipped: Some(0),
                slot: 0,
                hidden: vec![(0, 3)],
                appraised: true,
                durability: (90, 120),
                crafter: Some("kaio".into()),
                rarity: Some(1),
                mastery: 12,
                power: Some(SavedPower::Technique("double_cut".into())),
            }],
            recipes: vec!["iron_sword".into()],
            hp: 40,
            locked_until: 99,
            shop_sales: vec![("village_store".into(), 1200, 1_700_000_100)],
        };
        let (v, bytes) = encode(&s).unwrap();
        assert_eq!(v, 8);
        assert_eq!(decode(v, &bytes).unwrap(), s);
    }

    #[test]
    fn v2_rows_read_as_v3() {
        let old = postcard::to_stdvec(&SheetV2 {
            appearance: 2,
            coins: 5,
            backpack: vec![],
            quests: vec![],
            affinity: vec![],
        })
        .unwrap();
        let s = decode(2, &old).unwrap();
        assert_eq!((s.coins, s.progress.level, s.items.len()), (5, 0, 0));
    }
}
