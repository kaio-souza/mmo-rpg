//! Economy content (phase 7): shared loot tables and NPC shops.
//!
//! ```ron
//! // content/loot/beast_parts.ron
//! (coins: (2, 8), entries: [(item: "wolf_pelt", chance: 600, qty: (1, 2))])
//! // content/shops/village_store.ron
//! (price_permille: 1000, repairs: true,
//!  stock: [(item: "travel_bread", qty: 20, restock_ticks: 600)])
//! ```
//!
//! Prices come from the item's `value` and the numbers of
//! `content/economy/formulas.ron`; a shop only varies them by city
//! (`price_permille`) and says whether it repairs

use crate::npc::valid_id;
use crate::progress::{LootDef, LootFile};
use base::ItemId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A loot table several monsters may share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LootTableDef {
    pub id: String,
    /// Copper, added to what the monster drops by itself.
    pub coins: (u32, u32),
    pub entries: Vec<LootDef>,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct LootTableFile {
    #[serde(default)]
    pub coins: (u32, u32),
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<LootFile>,
}

pub(crate) fn parse_loot_table(id: &str, text: &str) -> Result<LootTableFile, String> {
    if !valid_id(id) {
        return Err(format!("loot table id `{id}` must be [a-z0-9_]"));
    }
    let f: LootTableFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if f.coins.0 > f.coins.1 {
        return Err("coins: min <= max".into());
    }
    crate::progress::check_loot(&f.entries)?;
    Ok(f)
}

/// One line of a shop's stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShopStock {
    pub item: ItemId,
    /// Pieces for sale; 0 is unlimited.
    pub qty: u32,
    /// Ticks to put one piece back (0: never restocks).
    pub restock_ticks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopDef {
    pub id: String,
    /// Price variation of this city, in thousandths of the item's value
    /// (: ±20%).
    pub price_permille: u32,
    /// The shop repairs equipment (at the cost of maximum durability).
    pub repairs: bool,
    /// Alpha: the shop appraises pieces for coins.
    pub appraises: bool,
    /// Alpha: coins the shop pays each player before it refills, and the
    /// real seconds it takes to refill; `None`: no limit.
    pub purse: Option<Purse>,
    pub stock: Vec<ShopStock>,
}

/// What a shop pays one player per period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Purse {
    /// Copper.
    pub coins: u64,
    /// Real seconds (a purse outlives a logout).
    pub refill_secs: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ShopStockFile {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
    pub item: String,
    /// 0: unlimited.
    #[serde(default)]
    pub qty: u32,
    #[serde(default)]
    pub restock_ticks: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ShopFile {
    #[serde(default = "thousand")]
    pub price_permille: u32,
    #[serde(default)]
    pub repairs: bool,
    #[serde(default)]
    pub appraises: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purse: Option<Purse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stock: Vec<ShopStockFile>,
}

fn thousand() -> u32 {
    1000
}

/// Widest price swing a city may have: ±50% of the item's value.
pub const PRICE_PERMILLE: (u32, u32) = (500, 1500);

pub(crate) fn parse_shop(id: &str, text: &str) -> Result<ShopFile, String> {
    if !valid_id(id) {
        return Err(format!("shop id `{id}` must be [a-z0-9_]"));
    }
    let f: ShopFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if !(PRICE_PERMILLE.0..=PRICE_PERMILLE.1).contains(&f.price_permille) {
        return Err(format!(
            "price_permille must be {} to {}",
            PRICE_PERMILLE.0, PRICE_PERMILLE.1
        ));
    }
    let mut seen: Vec<&str> = Vec::new();
    for s in &f.stock {
        if seen.contains(&s.item.as_str()) {
            return Err(format!("stock: item `{}` twice", s.item));
        }
        seen.push(&s.item);
    }
    Ok(f)
}

pub(crate) fn resolve_stock(
    file: Vec<ShopStockFile>,
    items: &BTreeMap<String, ItemId>,
) -> Result<Vec<ShopStock>, String> {
    file.into_iter()
        .map(|s| match items.get(&s.item) {
            Some(&item) => Ok(ShopStock {
                item,
                qty: s.qty,
                restock_ticks: s.restock_ticks,
            }),
            None => Err(format!("stock: item `{}` not found", s.item)),
        })
        .collect()
}

/// Price the player pays for one piece, in copper.
pub fn buy_price(value: u64, sell_permille: u32, price_permille: u32) -> u64 {
    permille(permille(value, sell_permille), price_permille).max(1)
}

/// Price the shop pays the player for one piece, in copper.
pub fn sell_price(value: u64, buy_permille: u32, price_permille: u32) -> u64 {
    permille(permille(value, buy_permille), price_permille)
}

/// Cost of a full repair of an item worth `value` missing `missing` of
/// `max` durability.
pub fn repair_price(value: u64, missing: u32, max: u32, repair_permille: u32, price_permille: u32) -> u64 {
    if max == 0 || missing == 0 {
        return 0;
    }
    let share = value.saturating_mul(u64::from(missing.min(max))) / u64::from(max);
    permille(permille(share, repair_permille), price_permille).max(1)
}

fn permille(value: u64, permille: u32) -> u64 {
    value.saturating_mul(u64::from(permille)) / 1000
}

/// Shared loot tables, checked (`content/loot/<id>.ron`).
pub(crate) fn validate_tables(
    files: &BTreeMap<String, String>,
    items: &BTreeMap<String, ItemId>,
    mut err: impl FnMut(String, String),
) -> Vec<LootTableDef> {
    let mut out = Vec::with_capacity(files.len());
    for (id, text) in files {
        let path = format!("loot/{id}.ron");
        let f = match parse_loot_table(id, text) {
            Ok(f) => f,
            Err(m) => {
                err(path, m);
                continue;
            }
        };
        let entries = match crate::progress::resolve_loot(f.entries, items) {
            Ok(e) => e,
            Err(m) => {
                err(path, m);
                continue;
            }
        };
        out.push(LootTableDef {
            id: id.clone(),
            coins: f.coins,
            entries,
        });
    }
    out
}

/// NPC shops, checked (`content/shops/<id>.ron`).
pub(crate) fn validate_shops(
    files: &BTreeMap<String, String>,
    items: &BTreeMap<String, ItemId>,
    item_defs: &[crate::ItemDef],
    mut err: impl FnMut(String, String),
) -> Vec<ShopDef> {
    let mut out = Vec::with_capacity(files.len());
    for (id, text) in files {
        let path = format!("shops/{id}.ron");
        let f = match parse_shop(id, text) {
            Ok(f) => f,
            Err(m) => {
                err(path, m);
                continue;
            }
        };
        let stock = match resolve_stock(f.stock, items) {
            Ok(s) => s,
            Err(m) => {
                err(path.clone(), m);
                continue;
            }
        };
        if f.purse.is_some_and(|p| p.coins == 0 || p.refill_secs == 0) {
            err(
                path.clone(),
                "purse: coins and refill_secs must be above 0".to_owned(),
            );
        }
        // A price of zero would be a free item: say so at validation, not
        // in play.
        for s in &stock {
            if item_defs.get(s.item.index()).is_some_and(|d| d.value == 0) {
                err(
                    path.clone(),
                    format!(
                        "stock: item `{}` has value 0: give it a value before selling it",
                        item_defs.get(s.item.index()).map_or("", |d| d.id.as_str())
                    ),
                );
            }
        }
        out.push(ShopDef {
            id: id.clone(),
            price_permille: f.price_permille,
            repairs: f.repairs,
            appraises: f.appraises,
            purse: f.purse,
            stock,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_follow_the_design() {
        // Sells at 100% of the value, buys at 30%.
        assert_eq!(buy_price(1000, 1000, 1000), 1000);
        assert_eq!(sell_price(1000, 300, 1000), 300);
        // A city 20% dearer.
        assert_eq!(buy_price(1000, 1000, 1200), 1200);
        // Half the durability of a 1000 item at 50%: 250.
        assert_eq!(repair_price(1000, 60, 120, 500, 1000), 250);
        assert_eq!(repair_price(1000, 0, 120, 500, 1000), 0);
        // Never free while something is missing.
        assert_eq!(repair_price(1, 1, 120, 500, 1000), 1);
    }

    #[test]
    fn a_shop_refuses_a_repeated_item_and_a_wild_price() {
        assert!(parse_shop("s", "(stock: [(item: \"a\"), (item: \"a\")])").is_err());
        assert!(parse_shop("s", "(price_permille: 5000)").is_err());
        let ok = parse_shop("s", "(repairs: true, stock: [(item: \"a\", qty: 2)])").unwrap();
        assert_eq!(ok.price_permille, 1000);
        assert!(ok.repairs);
    }
}
