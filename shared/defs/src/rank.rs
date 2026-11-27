//! Public rankings (`content/rankings/<id>.ron`): the best characters of the
//! server by one measure, read by players with `/ranking`.
//!
//! ```ron
//! // content/rankings/hunters.ron
//! (by: Counter(family: "kills", key: ""), size: 10)
//! ```
//!
//! The name is the text `ranking.<id>.name`. The gateway builds every
//! ranking from the saved sheets every few minutes; deleted characters and
//! characters with zero are left out.

use crate::npc::valid_id;
use base::TextId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Longest ranking (lines shown).
pub const MAX_RANKING_SIZE: u8 = 50;

/// What a ranking measures. New measures are appended.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum RankBy {
    /// Level, then XP.
    Level,
    /// A declared counter (`counters.ron`). `key` empty: the family total.
    Counter {
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::CounterFamilyRef"))]
        family: String,
        #[cfg_attr(feature = "editor", schemars(with = "crate::editor::CounterKeyRef"))]
        #[serde(default)]
        key: String,
    },
    /// Points of a proficiency.
    Proficiency(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::ProficiencyRef"))] String),
    /// Number of titles owned.
    Titles,
    /// Fastest clears of a dungeon (lowest time first), by party.
    FastestClear(#[cfg_attr(feature = "editor", schemars(with = "crate::editor::DungeonRef"))] String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankingDef {
    pub id: String,
    /// `ranking.<id>.name`.
    pub name: TextId,
    pub by: RankBy,
    /// Lines shown, 1 to `MAX_RANKING_SIZE`.
    pub size: u8,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct RankingFile {
    pub by: RankBy,
    /// Lines shown (1 to 50).
    #[serde(default = "default_size")]
    pub size: u8,
}

fn default_size() -> u8 {
    10
}

pub fn ranking_name_key(id: &str) -> String {
    format!("ranking.{id}.name")
}

pub(crate) fn parse_ranking(id: &str, text: &str) -> Result<RankingFile, String> {
    if !valid_id(id) {
        return Err(format!("ranking id `{id}` must be [a-z0-9_]"));
    }
    let f: RankingFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if f.size == 0 || f.size > MAX_RANKING_SIZE {
        return Err(format!("size must be 1 to {MAX_RANKING_SIZE}"));
    }
    Ok(f)
}

/// Rankings, checked, sorted by id. `counters` are the families declared;
/// `has_key(family, key)` says whether a counter key names something.
pub(crate) fn validate_rankings(
    files: &BTreeMap<String, String>,
    keys: &i18n::KeyTable,
    counters: &[String],
    proficiencies: &BTreeMap<String, base::ProficiencyId>,
    has_key: impl Fn(&str, &str) -> bool,
    mut err: impl FnMut(String, String),
) -> Vec<RankingDef> {
    let mut out = Vec::with_capacity(files.len());
    for (id, text) in files {
        let path = format!("rankings/{id}.ron");
        let f = match parse_ranking(id, text) {
            Ok(f) => f,
            Err(m) => {
                err(path, m);
                continue;
            }
        };
        match &f.by {
            RankBy::Counter { family, key } => {
                if !counters.contains(family) {
                    err(
                        path.clone(),
                        format!("counter family `{family}` is not declared in counters.ron"),
                    );
                } else if !key.is_empty() && !has_key(family, key) {
                    err(path.clone(), format!("counter `{family}` has no key `{key}`"));
                }
            }
            RankBy::Proficiency(p) if !proficiencies.contains_key(p) => {
                err(path.clone(), format!("proficiency `{p}` not found"));
            }
            RankBy::FastestClear(d) if !has_key("clears", d) => {
                err(path.clone(), format!("dungeon `{d}` not found"));
            }
            _ => {}
        }
        let Some(name) = keys.id(&ranking_name_key(id)) else {
            err(path, format!("text `{}` missing", ranking_name_key(id)));
            continue;
        };
        out.push(RankingDef {
            id: id.clone(),
            name,
            by: f.by,
            size: f.size,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_checks_size() {
        assert!(parse_ranking("hunters", "(by: Level)").is_ok_and(|f| f.size == 10));
        assert!(parse_ranking("hunters", "(by: Titles, size: 0)").is_err());
        assert!(parse_ranking("hunters", "(by: Titles, size: 51)").is_err());
        assert!(parse_ranking("Bad", "(by: Level)").is_err());
        let f = parse_ranking("x", r#"(by: Counter(family: "kills"))"#).unwrap();
        assert_eq!(
            f.by,
            RankBy::Counter {
                family: "kills".into(),
                key: String::new()
            }
        );
    }
}
