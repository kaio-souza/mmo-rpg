//! References to translated text. The simulation, events and protocol carry
//! these instead of sentences; `i18n` formats them.

use crate::Fixed;
use smallvec::SmallVec;

/// Index of a text key in the sorted key list of the content (`KeyTable`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct TextId(u32);

impl TextId {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A text to show, with its arguments.
///
/// Arguments are positional: position `i` fills the `i`-th distinct
/// placeholder name in order of first appearance in the source (`en`) text.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextRef {
    pub id: TextId,
    pub args: SmallVec<[Arg; 2]>,
}

impl TextRef {
    pub fn new(id: TextId) -> Self {
        Self {
            id,
            args: SmallVec::new(),
        }
    }

    pub fn arg(mut self, arg: Arg) -> Self {
        self.args.push(arg);
        self
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Arg {
    /// Literal value that is not translated (player name, item name given by its creator).
    Str(String),
    Int(i64),
    Fixed(Fixed),
    /// Another translated text, formatted in the same locale.
    Ref(Box<TextRef>),
    /// Secret text already resolved by the server in the player's locale.
    Resolved(String),
}
