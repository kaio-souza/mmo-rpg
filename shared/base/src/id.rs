//! Typed IDs. A raw `u32` never crosses a crate boundary.

macro_rules! id_types {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        pub struct $name(u32);

        impl $name {
            pub const fn new(raw: u32) -> Self {
                Self(raw)
            }

            pub const fn get(self) -> u32 {
                self.0
            }

            /// Position in a dense vector (content IDs are dense indices).
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }
    )*};
}

id_types! {
    /// An entity inside one zone simulation.
    EntityId,
    /// A connected player session.
    PlayerId,
    /// A saved character.
    CharacterId,
    /// A simulation zone.
    ZoneId,
    /// A login account.
    AccountId,
    ProficiencyId,
    TechniqueId,
    ClassId,
    TitleId,
    ItemId,
    NpcId,
    DialogueId,
    QuestId,
    RegionId,
    SpriteId,
    /// A monster definition (`content/monsters/<id>.ron`).
    MonsterId,
    /// A shared loot table (`content/loot/<id>.ron`, phase 7).
    LootTableId,
    /// An NPC shop (`content/shops/<id>.ron`, phase 7).
    ShopId,
    /// A crafting recipe (`content/recipes/<id>.ron`, phase 8).
    RecipeId,
    /// A gathering node (`content/nodes/<id>.ron`, phase 8).
    NodeId,
    /// An emote (`content/emotes/<id>.ron`, phase 9).
    EmoteId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let id = ClassId::new(7);
        assert_eq!(id.get(), 7);
        assert_eq!(id.index(), 7);
        assert!(ClassId::new(1) < ClassId::new(2));
    }
}
