//! Common value kinds.

/// STR, VIT, AGI, DEX, INT, SPI.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Attribute {
    Str,
    Vit,
    Agi,
    Dex,
    Int,
    Spi,
}

impl Attribute {
    pub const ALL: [Attribute; 6] = [
        Attribute::Str,
        Attribute::Vit,
        Attribute::Agi,
        Attribute::Dex,
        Attribute::Int,
        Attribute::Spi,
    ];
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Resource {
    Hp,
    Mp,
    Vigor,
}

impl Resource {
    pub const ALL: [Resource; 3] = [Resource::Hp, Resource::Mp, Resource::Vigor];
}

/// What a player chose at character creation:
/// dense positions in the lists of `content/appearance/appearance.ron`.
/// Saves name the options by text id; only the wire and the client use
/// these numbers. A position past the end of its list draws the first option.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Look {
    pub body: u8,
    pub skin: u8,
    pub hair_style: u8,
    pub hair_color: u8,
}
