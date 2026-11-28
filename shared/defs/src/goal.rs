//! Server goals (`content/goals/<id>.ron`): one shared quest for the whole
//! server. Every player adds to it by doing what a declared counter counts
//! (kills of a monster, dungeon clears, deaths); the gateway sums the zones
//! and, once `target` is reached, tells everyone and the goal is done for
//! good. Dialogues open with `GoalDone("<id>")`; players see it with `/goals`.
//!
//! ```ron
//! // content/goals/wolf_cull.ron
//! (family: "kills", key: "wolf", target: 5000)
//! ```
//!
//! Texts: `goal.<id>.name` and `goal.<id>.desc`.

use crate::npc::valid_id;
use base::TextId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalDef {
    pub id: String,
    /// `goal.<id>.name`.
    pub name: TextId,
    /// `goal.<id>.desc`.
    pub description: TextId,
    /// A declared counter family (`counters.ron`).
    pub family: String,
    /// A key of the family; empty: any key counts.
    pub key: String,
    /// What the whole server must reach.
    pub target: u64,
}

impl GoalDef {
    /// Whether one count of `(family, key)` adds to this goal.
    pub fn counts(&self, family: &str, key: &str) -> bool {
        self.family == family && (self.key.is_empty() || self.key == key)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct GoalFile {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::CounterFamilyRef"))]
    pub family: String,
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::CounterKeyRef"))]
    #[serde(default)]
    pub key: String,
    pub target: u64,
}

pub fn goal_name_key(id: &str) -> String {
    format!("goal.{id}.name")
}

pub fn goal_desc_key(id: &str) -> String {
    format!("goal.{id}.desc")
}

pub(crate) fn parse_goal(id: &str, text: &str) -> Result<GoalFile, String> {
    if !valid_id(id) {
        return Err(format!("goal id `{id}` must be [a-z0-9_]"));
    }
    let f: GoalFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if f.target == 0 {
        return Err("target must be at least 1".into());
    }
    Ok(f)
}

/// Goals, checked, sorted by id. `counters` are the families declared;
/// `has_key(family, key)` says whether a counter key names something.
pub(crate) fn validate_goals(
    files: &BTreeMap<String, String>,
    keys: &i18n::KeyTable,
    counters: &[String],
    has_key: impl Fn(&str, &str) -> bool,
    mut err: impl FnMut(String, String),
) -> Vec<GoalDef> {
    let mut out = Vec::with_capacity(files.len());
    for (id, text) in files {
        let path = format!("goals/{id}.ron");
        let f = match parse_goal(id, text) {
            Ok(f) => f,
            Err(m) => {
                err(path, m);
                continue;
            }
        };
        if !counters.contains(&f.family) {
            err(
                path.clone(),
                format!("counter family `{}` is not declared in counters.ron", f.family),
            );
        } else if !f.key.is_empty() && !has_key(&f.family, &f.key) {
            err(
                path.clone(),
                format!("counter `{}` has no key `{}`", f.family, f.key),
            );
        }
        let (Some(name), Some(description)) = (keys.id(&goal_name_key(id)), keys.id(&goal_desc_key(id)))
        else {
            err(
                path,
                format!("text `{}` or `{}` missing", goal_name_key(id), goal_desc_key(id)),
            );
            continue;
        };
        out.push(GoalDef {
            id: id.clone(),
            name,
            description,
            family: f.family,
            key: f.key,
            target: f.target,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_checks_target() {
        let f = parse_goal("cull", r#"(family: "kills", key: "wolf", target: 10)"#).unwrap();
        assert_eq!(
            (f.family.as_str(), f.key.as_str(), f.target),
            ("kills", "wolf", 10)
        );
        assert!(parse_goal("cull", r#"(family: "kills", target: 0)"#).is_err());
        assert!(parse_goal("Bad", r#"(family: "kills", target: 1)"#).is_err());
    }

    #[test]
    fn an_empty_key_counts_the_family() {
        let g = GoalDef {
            id: "deeds".into(),
            name: TextId::new(0),
            description: TextId::new(0),
            family: "kills".into(),
            key: String::new(),
            target: 1,
        };
        assert!(g.counts("kills", "bat") && !g.counts("deaths", ""));
    }
}
