//! Item identity.

/// Unique ID of a non-stackable item on the server.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ItemInstanceId(u64);

impl ItemInstanceId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// What an item is for. New kinds are appended: the order is part of the
/// wire format.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ItemKind {
    Material,
    Consumable,
    /// Quest items: delivered by quests, usually bound to the character.
    Quest,
    /// Equipment (phase 4): one per slot, each with its own `ItemInstanceId`.
    Weapon,
    Armor,
    /// Phase 11: held in the other hand (a torch).
    Offhand,
}

/// Item rarity, lowest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Rarity {
    #[default]
    Common,
    Rare,
    Epic,
    Unique,
    Legendary,
    Mythic,
}

/// A power a piece of equipment may carry from Unique up
/// a technique the wearer can
/// use while it is equipped, or an attribute bonus while it is equipped.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ItemPower {
    Technique(crate::TechniqueId),
    Attribute(crate::Attribute, u16),
}

/// A range of item IDs reserved in the database for one zone (`[next, end)`).
/// The zone hands it to the simulation as input, keeping it deterministic.
///
/// Not `Clone`: two holders of the same block would hand out the same IDs.
#[derive(PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IdBlock {
    next: u64,
    end: u64,
}

impl IdBlock {
    /// Empty when `end <= start`.
    pub const fn new(start: u64, end: u64) -> Self {
        Self { next: start, end }
    }

    /// `(next, end)`: to save the block in a snapshot.
    pub const fn bounds(&self) -> (u64, u64) {
        (self.next, self.end)
    }

    pub const fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.next)
    }

    pub fn take(&mut self) -> Option<ItemInstanceId> {
        if self.next >= self.end {
            return None;
        }
        let id = ItemInstanceId(self.next);
        self.next += 1;
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_hands_out_each_id_once() {
        let mut b = IdBlock::new(10, 13);
        assert_eq!(b.remaining(), 3);
        let ids: Vec<_> = std::iter::from_fn(|| b.take()).map(|i| i.get()).collect();
        assert_eq!(ids, [10, 11, 12]);
        assert_eq!(b.remaining(), 0);
        assert_eq!(IdBlock::new(5, 2).take(), None);
    }
}
