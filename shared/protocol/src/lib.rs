//! Messages between client and server and how they become bytes: the
//! contract between both sides.
//!
//! Message lists:
//! `protocolo-fase2.md`, `protocolo-fase3.md`.
//!
//! Frame: `u32` little endian length, then the `postcard` payload.
//! Decoding untrusted bytes never panics: bad input is a `ProtocolError`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod account;
mod chunk;
mod frame;
mod msg;
mod wire_locale;

pub use account::{
    CharacterSummary, DeviceToken, InvalidPassword, Password, Preference, SessionToken, proof_ok, solve_proof,
};
pub use chunk::{
    CHUNK_MARGIN, CHUNK_SIZE, CHUNK_TILES, ChunkTiles, ChunkTilesError, KEEP_CHUNKS, TILE_BLOCKS, TILE_FLOOR,
    TILE_HIDES, TILE_SWITCH, chunk_kept, chunk_of, sight_line,
};
pub use frame::{
    EncodedEntity, MAX_CLIENT_FRAME, MAX_SERVER_FRAME, ProtocolError, decode_frame, encode, encode_diff,
    encode_entity, encode_snapshot,
};
pub use msg::{
    AdminCommand, AppearanceInfo, ChatChannel, ClientMsg, CombatEvent, Diff, Entity, EntityKind, EntityName,
    EquipSlot, ErrorCode, InstanceInfo, InviteKind, ItemInfo, ItemSlot, LightInfo, LookOptionInfo, NamedInfo,
    ObjectiveView, PartyView, QuestEntry, QuestMarker, QuestState, RankingId, ReasonId, RecipeInfo,
    ServerMsg, ShopLine, ShopView, StatsInfo, TechniqueInfo, TechniqueUse, TitleBonusInfo, TradeView,
    UseInfo, ZoneInfo,
};

/// Incompatible changes bump this; the server refuses other versions.
pub const PROTOCOL_VERSION: u16 = 28;

/// Client messages per second per connection (`server-net` enforces it).
pub const MAX_CLIENT_MSGS_PER_SEC: u32 = 30;

/// Options of one dialogue node; the client numbers them 1 to 9 (`defs`
/// refuses more, `defs::MAX_OPTIONS`).
pub const MAX_DIALOGUE_OPTIONS: usize = 9;

/// Largest piece of a text pack in one `TextPack` frame, well under
/// `MAX_SERVER_FRAME`.
pub const TEXT_PACK_PART: usize = 512 * 1024;

/// Cuts a compressed text pack into `TextPack` messages, in order.
pub fn text_pack_parts(
    locale: base::Locale,
    content_hash: u64,
    data: &[u8],
    part_size: usize,
) -> Vec<ServerMsg> {
    let chunks: Vec<&[u8]> = if data.is_empty() {
        vec![data]
    } else {
        data.chunks(part_size.max(1)).collect()
    };
    let parts = u16::try_from(chunks.len()).unwrap_or(u16::MAX);
    chunks
        .into_iter()
        .take(usize::from(parts))
        .enumerate()
        .map(|(i, c)| ServerMsg::TextPack {
            locale,
            content_hash,
            part: u16::try_from(i).unwrap_or(u16::MAX),
            parts,
            data: c.to_vec(),
        })
        .collect()
}

/// Farthest `Move` destination, in tiles (`server` enforces it).
pub const MAX_MOVE_DISTANCE: u32 = 32;
