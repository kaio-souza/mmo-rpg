//! Dungeons (`content/dungeons/<id>.ron`): the rules of an instance zone.
//!
//! ```ron
//! // content/dungeons/crypt.ron
//! (zone: "crypt", party: (1, 6), level: (5, 30), boss: "crypt_lord", lockout: Daily)
//! ```
//!
//! The name is the text `dungeon.<id>.name`. Entering (a portal or a
//! `Teleport` into the zone) needs the level range and a group of the
//! party size; killing the boss clears the run: every player inside counts
//! a `clears` (key: the dungeon), the time is recorded for the fastest
//! clears (`RankBy::FastestClear`) and, with a lockout, they cannot enter
//! again until the next reset.

use crate::npc::valid_id;
use base::{MonsterId, TextId, ZoneId};
use serde::{Deserialize, Serialize};

/// When characters who cleared a dungeon may enter it again. New variants
/// are appended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum Lockout {
    /// Any time.
    #[default]
    None,
    /// After the next midnight (UTC).
    Daily,
    /// After the next Monday midnight (UTC).
    Weekly,
}

impl Lockout {
    /// Unix second the lockout of a clear at `now` ends (`now` for `None`).
    pub fn until(self, now: u64) -> u64 {
        const DAY: u64 = 86_400;
        let next_day = (now / DAY + 1) * DAY;
        match self {
            Lockout::None => now,
            Lockout::Daily => next_day,
            // 1970-01-01 was a Thursday: day 4 is the first Monday.
            Lockout::Weekly => {
                let days = now / DAY;
                let to_monday = (7 - (days + 3) % 7) % 7;
                let to_monday = if to_monday == 0 { 7 } else { to_monday };
                (days + to_monday) * DAY
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DungeonDef {
    pub id: String,
    /// `dungeon.<id>.name`.
    pub name: TextId,
    /// The instance zone it runs in.
    pub zone: ZoneId,
    /// Group size to enter, inclusive (alone is 1).
    pub party: (u8, u8),
    /// Character level to enter, inclusive.
    pub level: (u8, u8),
    /// Killing it clears the run.
    pub boss: Option<MonsterId>,
    pub lockout: Lockout,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct DungeonFile {
    /// The zone (with `instance`) it runs in.
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::ZoneRef"))]
    pub zone: String,
    /// Group size to enter, min and max (1: alone).
    #[serde(default = "party")]
    pub party: (u8, u8),
    /// Character level to enter, min and max.
    #[serde(default = "levels")]
    pub level: (u8, u8),
    /// The monster whose death clears the run (none: never cleared).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::MonsterRef"))]
    pub boss: String,
    /// When those who cleared it may enter again.
    #[serde(default)]
    pub lockout: Lockout,
}

fn party() -> (u8, u8) {
    (1, 6)
}

fn levels() -> (u8, u8) {
    (1, u8::MAX)
}

pub fn dungeon_name_key(id: &str) -> String {
    format!("dungeon.{id}.name")
}

/// Largest group a dungeon may ask for (a group holds 6).
pub const MAX_DUNGEON_PARTY: u8 = 6;

pub(crate) fn parse_dungeon(id: &str, text: &str) -> Result<DungeonFile, String> {
    if !valid_id(id) {
        return Err(format!("dungeon id `{id}` must be [a-z0-9_]"));
    }
    let f: DungeonFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if f.party.0 == 0 || f.party.0 > f.party.1 || f.party.1 > MAX_DUNGEON_PARTY {
        return Err(format!("party: 1 <= min <= max <= {MAX_DUNGEON_PARTY}"));
    }
    if f.level.0 == 0 || f.level.0 > f.level.1 {
        return Err("level: 1 <= min <= max".into());
    }
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockouts_end_at_midnight_and_on_monday() {
        // 2026-11-27 is a Friday; 10:00 UTC.
        let fri = 1_795_773_600;
        assert_eq!(Lockout::None.until(fri), fri);
        assert_eq!(Lockout::Daily.until(fri), 1_795_824_000, "Saturday 00:00");
        assert_eq!(Lockout::Weekly.until(fri), 1_795_996_800, "Monday 00:00");
        // On a Monday, the next Monday.
        assert_eq!(
            Lockout::Weekly.until(1_795_996_800 + 5),
            1_795_996_800 + 7 * 86_400
        );
    }

    #[test]
    fn checks_ranges() {
        assert!(parse_dungeon("crypt", r#"(zone: "crypt")"#).is_ok());
        assert!(parse_dungeon("crypt", r#"(zone: "crypt", party: (3, 2))"#).is_err());
        assert!(parse_dungeon("crypt", r#"(zone: "crypt", party: (1, 7))"#).is_err());
        assert!(parse_dungeon("crypt", r#"(zone: "crypt", level: (0, 5))"#).is_err());
    }
}
