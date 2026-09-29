use crate::msg::{DIFF_VARIANT, Entity, SNAPSHOT_VARIANT};
use base::{Direction, EntityId, TilePos};
use bytes::Bytes;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt;

/// Largest client frame payload (phase 1 rule: 1 KiB).
pub const MAX_CLIENT_FRAME: usize = 1024;
/// Largest server frame payload.
pub const MAX_SERVER_FRAME: usize = 1024 * 1024;

const PREFIX: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    TooLarge {
        size: usize,
        max: usize,
    },
    Decode(String),
    /// The payload decoded but left bytes unread.
    TrailingBytes,
    Encode(String),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { size, max } => write!(f, "frame of {size} bytes, limit {max}"),
            Self::Decode(e) => write!(f, "decode: {e}"),
            Self::TrailingBytes => f.write_str("frame has trailing bytes"),
            Self::Encode(e) => write!(f, "encode: {e}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Appends one frame (length prefix + payload) to `buf`.
pub fn encode<M: Serialize>(msg: &M, buf: &mut Vec<u8>) -> Result<(), ProtocolError> {
    let start = buf.len();
    buf.extend_from_slice(&[0; PREFIX]);
    let mut out = std::mem::take(buf);
    out = postcard::to_extend(msg, out).map_err(|e| ProtocolError::Encode(e.to_string()))?;
    *buf = out;
    write_prefix(buf, start)
}

fn write_prefix(buf: &mut [u8], start: usize) -> Result<(), ProtocolError> {
    let len = buf.len() - start - PREFIX;
    let len = u32::try_from(len).map_err(|_| ProtocolError::Encode("frame over 4 GiB".into()))?;
    if let Some(p) = buf.get_mut(start..start + PREFIX) {
        p.copy_from_slice(&len.to_le_bytes());
    }
    Ok(())
}

/// Reads one frame from the start of `buf`.
///
/// `Ok(None)`: not enough bytes yet. `Ok(Some((msg, used)))`: `used` bytes
/// consumed. An error means the connection must be closed. The size check
/// happens before waiting for the payload, so a huge prefix fails at once.
pub fn decode_frame<M: DeserializeOwned>(
    buf: &[u8],
    max: usize,
) -> Result<Option<(M, usize)>, ProtocolError> {
    let Some(prefix) = buf.get(..PREFIX).and_then(|p| <[u8; PREFIX]>::try_from(p).ok()) else {
        return Ok(None);
    };
    let size = u32::from_le_bytes(prefix) as usize;
    if size > max {
        return Err(ProtocolError::TooLarge { size, max });
    }
    let Some(payload) = buf.get(PREFIX..PREFIX + size) else {
        return Ok(None);
    };
    let (msg, rest) = postcard::take_from_bytes(payload).map_err(|e| ProtocolError::Decode(e.to_string()))?;
    if !rest.is_empty() {
        return Err(ProtocolError::TrailingBytes);
    }
    Ok(Some((msg, PREFIX + size)))
}

/// One entity encoded once per tick and shared by every `Diff` that has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedEntity(Bytes);

pub fn encode_entity(e: &Entity) -> Result<EncodedEntity, ProtocolError> {
    postcard::to_stdvec(e)
        .map(|v| EncodedEntity(Bytes::from(v)))
        .map_err(|err| ProtocolError::Encode(err.to_string()))
}

/// Appends a `ServerMsg::Diff` frame built from entities already encoded.
/// Byte for byte the same as `encode(&ServerMsg::Diff(..))`.
pub fn encode_diff(
    tick: u32,
    entered: &[EncodedEntity],
    left: &[EntityId],
    moved: &[(EntityId, TilePos, Direction)],
    updated: &[EncodedEntity],
    buf: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    let start = buf.len();
    buf.extend_from_slice(&[0; PREFIX]);
    varint(u64::from(DIFF_VARIANT), buf);
    varint(u64::from(tick), buf);
    varint(entered.len() as u64, buf);
    for e in entered {
        buf.extend_from_slice(&e.0);
    }
    let mut out = std::mem::take(buf);
    out = postcard::to_extend(&(left, moved), out).map_err(|e| ProtocolError::Encode(e.to_string()))?;
    *buf = out;
    varint(updated.len() as u64, buf);
    for e in updated {
        buf.extend_from_slice(&e.0);
    }
    write_prefix(buf, start)
}

/// Appends a `ServerMsg::Snapshot` frame built from entities already encoded.
pub fn encode_snapshot(entities: &[EncodedEntity], buf: &mut Vec<u8>) -> Result<(), ProtocolError> {
    let start = buf.len();
    buf.extend_from_slice(&[0; PREFIX]);
    varint(u64::from(SNAPSHOT_VARIANT), buf);
    varint(entities.len() as u64, buf);
    for e in entities {
        buf.extend_from_slice(&e.0);
    }
    write_prefix(buf, start)
}

/// LEB128, as postcard writes unsigned integers and lengths.
fn varint(mut v: u64, buf: &mut Vec<u8>) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            buf.push(byte);
            return;
        }
        buf.push(byte | 0x80);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CHUNK_TILES, CharacterSummary, ChunkTiles, ClientMsg, Diff, EntityKind, EntityName, ErrorCode,
        Password, Preference, ServerMsg, SessionToken,
    };
    use base::{AccountName, Arg, CharacterId, CharacterName, Locale, TextId};

    fn entity(id: u32, x: i32) -> Entity {
        Entity {
            id: EntityId::new(id),
            kind: EntityKind::Player,
            pos: TilePos::new(x, -x),
            direction: Direction::SW,
            appearance: 300,
            name: EntityName::Player(CharacterName::try_from("Kai_99").unwrap()),
            health: 75,
            equipment: (3, 0, 0),
            light: 0,
            level: 12,
            look: base::Look {
                body: 1,
                skin: 5,
                hair_style: 7,
                hair_color: 9,
            },
        }
    }

    fn roundtrip<M: Serialize + DeserializeOwned + PartialEq + fmt::Debug>(m: M, max: usize) {
        let mut buf = Vec::new();
        encode(&m, &mut buf).unwrap();
        let (back, used): (M, usize) = decode_frame(&buf, max).unwrap().unwrap();
        assert_eq!(back, m);
        assert_eq!(used, buf.len());
    }

    #[test]
    fn every_message_roundtrips() {
        for m in [
            ClientMsg::Hello {
                version: 1,
                locale: Locale::from_static("pt-BR"),
            },
            ClientMsg::DevLogin {
                name: CharacterName::try_from("abc").unwrap(),
            },
            ClientMsg::Move {
                destination: TilePos::new(-5, 70000),
            },
            ClientMsg::Stop,
            ClientMsg::Ping { n: u32::MAX },
            ClientMsg::RequestChunk { cx: -1, cy: 2 },
            ClientMsg::Register {
                account: AccountName::try_from("kai").unwrap(),
                password: Password::try_from("x".repeat(128)).unwrap(),
                proof: u64::MAX,
            },
            ClientMsg::Login {
                account: AccountName::try_from("kai").unwrap(),
                password: Password::try_from("パスワードです！！".to_owned()).unwrap(),
                device: Some(crate::DeviceToken([9; 32])),
                proof: 7,
            },
            ClientMsg::Resume {
                token: SessionToken([0xab; 32]),
            },
            ClientMsg::CreateCharacter {
                name: CharacterName::try_from("Kai_99").unwrap(),
                look: base::Look::default(),
            },
            ClientMsg::Enter {
                character: CharacterId::new(u32::MAX),
            },
            ClientMsg::SetPreferences(Preference::Locale(Locale::from_static("ja"))),
            ClientMsg::MoveItem { from: 0, to: 19 },
            ClientMsg::Chat {
                channel: crate::ChatChannel::Whisper(CharacterName::try_from("Ana").unwrap()),
                text: base::ChatText::try_from("你好, Ana!").unwrap(),
            },
            ClientMsg::Admin(crate::AdminCommand::Mute {
                name: CharacterName::try_from("Ana").unwrap(),
                secs: 3600,
                reason: crate::ReasonId::try_from("spam").unwrap(),
            }),
            ClientMsg::ReportChat {
                name: CharacterName::try_from("Ana").unwrap(),
            },
        ] {
            roundtrip(m, MAX_CLIENT_FRAME);
        }
        for m in [
            ServerMsg::Welcome {
                your_id: EntityId::new(9),
                pos: TilePos::new(1, 2),
                tick: 7,
                content_hash: u64::MAX,
                view_radius: 20,
            },
            ServerMsg::Chunk {
                cx: 0,
                cy: -3,
                tiles: ChunkTiles::try_from(vec![0x8001; CHUNK_TILES]).unwrap(),
            },
            ServerMsg::Snapshot {
                entities: vec![entity(1, 5), entity(2, -5)],
            },
            ServerMsg::Diff(Diff {
                tick: 3,
                entered: vec![entity(1, 1)],
                left: vec![EntityId::new(4)],
                moved: vec![],
                updated: vec![entity(9, 2)],
            }),
            ServerMsg::Pong { n: 0 },
            ServerMsg::Error {
                code: ErrorCode::ZoneBusy,
                args: vec![
                    Arg::Int(3),
                    Arg::Ref(Box::new(base::TextRef::new(TextId::new(1)))),
                ],
            },
            ServerMsg::Session {
                token: SessionToken([1; 32]),
                characters: vec![CharacterSummary {
                    id: CharacterId::new(3),
                    name: CharacterName::try_from("Kai_99").unwrap(),
                    level: 4,
                    look: base::Look::default(),
                }],
                device: Some(crate::DeviceToken([2; 32])),
                failed_logins: 3,
            },
            ServerMsg::Challenge {
                seed: [5; 16],
                bits: 18,
            },
            ServerMsg::Characters { characters: vec![] },
            ServerMsg::ChatMessage {
                channel: crate::ChatChannel::Local,
                from: CharacterName::try_from("Kai_99").unwrap(),
                text: base::ChatText::try_from("olá").unwrap(),
            },
            ServerMsg::Appearances(crate::AppearanceInfo {
                lists: [
                    vec![crate::LookOptionInfo {
                        name: TextId::new(1),
                        color: 0xc08060,
                    }],
                    vec![],
                    vec![],
                    vec![],
                ],
            }),
            ServerMsg::QueuePosition {
                position: 12,
                estimate_secs: 90,
            },
            ServerMsg::TextPackChanged { content_hash: 5 },
            ServerMsg::Definitions {
                content_hash: 5,
                items: vec![crate::ItemInfo {
                    name: TextId::new(1),
                    description: TextId::new(2),
                    kind: base::ItemKind::Quest,
                    rarity: base::Rarity::Epic,
                    max_stack: 20,
                    weight: 50,
                    use_effect: None,
                    value: 1200,
                    durability: 0,
                    icon: 1,
                    weapon: Some(base::ProficiencyId::new(2)),
                    light: Some(crate::LightInfo {
                        radius: 5,
                        color: [255, 160, 70],
                    }),
                    burn: 1,
                }],
                classes: vec![crate::NamedInfo {
                    name: TextId::new(3),
                    description: Some(TextId::new(4)),
                    icon: 0,
                }],
                techniques: vec![],
                proficiencies: vec![],
                titles: vec![],
                sprites: vec!["wolf".into()],
                icons: vec!["glow_herb".into()],
                recipes: vec![crate::RecipeInfo {
                    name: TextId::new(5),
                    description: TextId::new(6),
                    proficiency: base::ProficiencyId::new(0),
                    min_points: 3,
                    materials: vec![(base::ItemId::new(0), 2)],
                    result: base::ItemId::new(1),
                    qty: 1,
                }],
                emotes: vec![TextId::new(7)],
                title_bonuses: vec![vec![crate::TitleBonusInfo::XpPercent(5, Some(10))]],
                emote_icons: vec![1],
                zones: vec![crate::ZoneInfo {
                    name: "aurelia".into(),
                    min: base::TilePos::new(-32, -96),
                    max: base::TilePos::new(31, -33),
                }],
            },
            ServerMsg::Combat {
                events: vec![crate::CombatEvent {
                    attacker: EntityId::new(1),
                    target: EntityId::new(2),
                    damage: Some(7),
                    crit: true,
                    killed: false,
                    technique: Some(base::TechniqueId::new(2)),
                }],
            },
            ServerMsg::TechniqueUsed {
                events: vec![crate::TechniqueUse {
                    user: EntityId::new(1),
                    target: EntityId::new(1),
                    technique: base::TechniqueId::new(3),
                }],
            },
            ServerMsg::Inventory {
                instances: vec![crate::InstanceInfo {
                    slot: 1,
                    id: 99,
                    hidden: Some(vec![(base::Attribute::Str, 3)]),
                    durability: (90, 120),
                    crafter: None,
                    rarity: base::Rarity::Rare,
                    power_permille: 100,
                    mastery: 12,
                    power: Some(base::ItemPower::Attribute(base::Attribute::Dex, 2)),
                }],
                equipped: vec![],
                coins: 150,
                slots: vec![
                    None,
                    Some(crate::ItemSlot {
                        item: base::ItemId::new(0),
                        qty: 3,
                    }),
                ],
            },
            ServerMsg::QuestLog {
                quests: vec![crate::QuestEntry {
                    name: TextId::new(3),
                    description: TextId::new(4),
                    state: crate::QuestState::Active,
                    goal: Some(TilePos::new(-4, 9)),
                    objectives: vec![
                        crate::ObjectiveView::Talk {
                            npc: TextId::new(5),
                            done: true,
                        },
                        crate::ObjectiveView::Item {
                            item: base::ItemId::new(1),
                            have: 1,
                            need: 3,
                        },
                    ],
                }],
            },
        ] {
            roundtrip(m, MAX_SERVER_FRAME);
        }
    }

    #[test]
    fn text_pack_parts_fit_a_frame_and_join() {
        let data: Vec<u8> = (0..(crate::TEXT_PACK_PART * 2 + 10)).map(|i| i as u8).collect();
        let parts = crate::text_pack_parts(Locale::from_static("de"), 9, &data, crate::TEXT_PACK_PART);
        assert_eq!(parts.len(), 3);
        let mut joined = Vec::new();
        for (i, m) in parts.iter().enumerate() {
            let mut buf = Vec::new();
            encode(m, &mut buf).unwrap();
            assert!(buf.len() <= MAX_SERVER_FRAME);
            let ServerMsg::TextPack {
                part, parts, data, ..
            } = m
            else {
                panic!()
            };
            assert_eq!((usize::from(*part), *parts), (i, 3));
            joined.extend_from_slice(data);
        }
        assert_eq!(joined, data);
        assert_eq!(
            crate::text_pack_parts(Locale::from_static("de"), 9, &[], 4).len(),
            1
        );
    }

    #[test]
    fn bad_account_fields_fail_to_decode() {
        #[derive(Serialize)]
        enum Raw {
            #[allow(dead_code)]
            Hello,
            #[allow(dead_code)]
            DevLogin,
            #[allow(dead_code)]
            Move,
            #[allow(dead_code)]
            Stop,
            #[allow(dead_code)]
            Ping,
            #[allow(dead_code)]
            RequestChunk,
            Register {
                account: String,
                password: String,
                proof: u64,
            },
        }
        for (account, password) in [("ok_name", "short"), ("x", "long enough"), ("a b", "long enough")] {
            let mut buf = Vec::new();
            encode(
                &Raw::Register {
                    account: account.into(),
                    password: password.into(),
                    proof: 0,
                },
                &mut buf,
            )
            .unwrap();
            assert!(
                decode_frame::<ClientMsg>(&buf, MAX_CLIENT_FRAME).is_err(),
                "{account}/{password}"
            );
        }
    }

    #[test]
    fn a_bad_locale_code_becomes_en() {
        // Hello { version: 1, locale: "x_Y" } written by hand: any well
        // formed code passes (the server may lack it), a bad one
        // is English.
        let payload = [0u8, 1, 3, b'x', b'_', b'Y'];
        let mut buf = (payload.len() as u32).to_le_bytes().to_vec();
        buf.extend_from_slice(&payload);
        let (m, _): (ClientMsg, _) = decode_frame(&buf, MAX_CLIENT_FRAME).unwrap().unwrap();
        assert_eq!(
            m,
            ClientMsg::Hello {
                version: 1,
                locale: Locale::EN
            }
        );
    }

    #[test]
    fn diff_from_pieces_equals_plain_encoding() {
        let es = [entity(1, 10), entity(70000, -3)];
        let left = [EntityId::new(5), EntityId::new(300)];
        let moved = [(EntityId::new(2), TilePos::new(-1, 99), Direction::N)];
        let mut plain = Vec::new();
        encode(
            &ServerMsg::Diff(Diff {
                tick: 1234,
                entered: es.to_vec(),
                left: left.to_vec(),
                moved: moved.to_vec(),
                updated: es[..1].to_vec(),
            }),
            &mut plain,
        )
        .unwrap();
        let pieces: Vec<_> = es.iter().map(|e| encode_entity(e).unwrap()).collect();
        let mut fast = Vec::new();
        encode_diff(1234, &pieces, &left, &moved, &pieces[..1], &mut fast).unwrap();
        assert_eq!(fast, plain);
    }

    #[test]
    fn snapshot_from_pieces_equals_plain_encoding() {
        let es = [entity(3, 1), entity(9, 2)];
        let mut plain = Vec::new();
        encode(
            &ServerMsg::Snapshot {
                entities: es.to_vec(),
            },
            &mut plain,
        )
        .unwrap();
        let pieces: Vec<_> = es.iter().map(|e| encode_entity(e).unwrap()).collect();
        let mut fast = Vec::new();
        encode_snapshot(&pieces, &mut fast).unwrap();
        assert_eq!(fast, plain);
    }

    #[test]
    fn partial_and_bad_frames() {
        let mut buf = Vec::new();
        encode(&ClientMsg::Ping { n: 5 }, &mut buf).unwrap();
        for cut in 0..buf.len() {
            assert_eq!(decode_frame::<ClientMsg>(&buf[..cut], MAX_CLIENT_FRAME), Ok(None));
        }
        // Huge prefix fails before the payload arrives.
        let huge = [0xff, 0xff, 0xff, 0x7f];
        assert!(matches!(
            decode_frame::<ClientMsg>(&huge, MAX_CLIENT_FRAME),
            Err(ProtocolError::TooLarge { .. })
        ));
        // Garbage, bad name, trailing bytes.
        for payload in [&[0xffu8, 0xff][..], &[1, 2, b'a', b' '], &[4, 1, 0]] {
            let mut b = (payload.len() as u32).to_le_bytes().to_vec();
            b.extend_from_slice(payload);
            assert!(
                decode_frame::<ClientMsg>(&b, MAX_CLIENT_FRAME).is_err(),
                "{payload:?}"
            );
        }
    }

    #[test]
    fn two_frames_in_one_buffer() {
        let mut buf = Vec::new();
        encode(&ClientMsg::Stop, &mut buf).unwrap();
        encode(&ClientMsg::Ping { n: 1 }, &mut buf).unwrap();
        let (a, used): (ClientMsg, _) = decode_frame(&buf, MAX_CLIENT_FRAME).unwrap().unwrap();
        let (b, _): (ClientMsg, _) = decode_frame(&buf[used..], MAX_CLIENT_FRAME).unwrap().unwrap();
        assert_eq!((a, b), (ClientMsg::Stop, ClientMsg::Ping { n: 1 }));
    }
}
