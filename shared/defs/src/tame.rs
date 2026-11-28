//! Taming: the `tame` block of a monster (`monsters/<id>.ron`). The
//! designer decides who may tame it with an open list of requirements, so a
//! game can ask for a taming skill, a class, a title, karma, a bait item or
//! any mix of them.
//!
//! ```ron
//! tame: Some((
//!     requires: [Proficiency("taming", 10), Item(item: "raw_meat", qty: 1)],
//!     below_hp_permille: 300,   // weakened to 30 % of its HP or less
//!     chance_permille: 500,     // half the attempts succeed
//! )),
//! ```
//!
//! A tamed monster becomes a **companion** of the character: it keeps the
//! monster's stats and art, follows its owner and helps against its target.

use base::{Attribute, ClassId, ItemId, ProficiencyId, TitleId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One requirement to tame, with content ids resolved. All of a monster's
/// list must hold. New kinds are appended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TameReq {
    /// The character's level, at least.
    Level(u8),
    /// A base attribute (without equipment), at least.
    Attribute(Attribute, u16),
    /// Points of a proficiency (a taming skill, a profession), at least.
    Proficiency(ProficiencyId, u8),
    /// Owning this title.
    Title(TitleId),
    /// Having this class (or one grown from it is not enough: this one).
    Class(ClassId),
    /// Karma at least this much; negative: at most (a dark tamer).
    Karma(i32),
    /// Holding these, taken on every attempt (a bait).
    Item { item: ItemId, qty: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tame {
    pub requires: Vec<TameReq>,
    /// The monster must be at or below this share of its HP (1000: any).
    pub below_hp_permille: u16,
    /// Chance of each attempt, in thousandths.
    pub chance_permille: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub(crate) enum TameReqFile {
    Level(u8),
    Attribute(Attribute, u16),
    Proficiency(
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ProficiencyRef"))] String,
        u8,
    ),
    Title(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::TitleRef"))] String),
    Class(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::ClassRef"))] String),
    Karma(i32),
    Item {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ItemRef"))]
        item: String,
        qty: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct TameFile {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<TameReqFile>,
    #[serde(default = "full")]
    pub below_hp_permille: u16,
    #[serde(default = "full")]
    pub chance_permille: u16,
}

fn full() -> u16 {
    1000
}

/// The ids a requirement may name.
pub(crate) struct TameNames<'a> {
    pub items: &'a BTreeMap<String, ItemId>,
    pub proficiencies: &'a BTreeMap<String, ProficiencyId>,
    pub titles: &'a BTreeMap<String, TitleId>,
    pub classes: &'a BTreeMap<String, ClassId>,
}

pub(crate) fn resolve(f: TameFile, names: &TameNames<'_>) -> Result<Tame, String> {
    if f.below_hp_permille == 0 || f.below_hp_permille > 1000 {
        return Err("tame: below_hp_permille must be 1 to 1000".into());
    }
    if f.chance_permille == 0 || f.chance_permille > 1000 {
        return Err("tame: chance_permille must be 1 to 1000".into());
    }
    let mut requires = Vec::with_capacity(f.requires.len());
    for r in f.requires {
        let missing = |kind: &str, id: &str| format!("tame: {kind} `{id}` not found");
        requires.push(match r {
            TameReqFile::Level(l) => TameReq::Level(l),
            TameReqFile::Attribute(a, v) => TameReq::Attribute(a, v),
            TameReqFile::Proficiency(p, v) => TameReq::Proficiency(
                *names
                    .proficiencies
                    .get(&p)
                    .ok_or_else(|| missing("proficiency", &p))?,
                v,
            ),
            TameReqFile::Title(t) => {
                TameReq::Title(*names.titles.get(&t).ok_or_else(|| missing("title", &t))?)
            }
            TameReqFile::Class(c) => {
                TameReq::Class(*names.classes.get(&c).ok_or_else(|| missing("class", &c))?)
            }
            TameReqFile::Karma(k) => {
                if !(-crate::KARMA_MAX..=crate::KARMA_MAX).contains(&k) {
                    return Err(format!("tame: karma {k} must be -{0} to {0}", crate::KARMA_MAX));
                }
                TameReq::Karma(k)
            }
            TameReqFile::Item { item, qty } => {
                if qty == 0 {
                    return Err(format!("tame: item `{item}`: qty must be at least 1"));
                }
                TameReq::Item {
                    item: *names.items.get(&item).ok_or_else(|| missing("item", &item))?,
                    qty,
                }
            }
        });
    }
    Ok(Tame {
        requires,
        below_hp_permille: f.below_hp_permille,
        chance_permille: f.chance_permille,
    })
}
