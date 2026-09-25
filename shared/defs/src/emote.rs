//! Emotes (phase 9): gestures a player triggers by name, shown over the
//! character for whoever sees it.
//!
//! ```ron
//! // content/emotes/wave.ron
//! (cooldown_ticks: 40, icon: "emote_wave")
//! ```
//!
//! The name is the text `emote.<id>.name`; the client matches what the
//! player types against it in the player's own language. `icon` (an icon
//! name, `assets/icons/`) is the picture shown in a bubble over the
//! character and in the emote list; empty shows the name instead.

use crate::npc::valid_id;
use base::TextId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Shortest wait between two emotes of one player, in ticks (2 s at 20 Hz).
pub const DEFAULT_EMOTE_COOLDOWN_TICKS: u32 = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmoteDef {
    pub id: String,
    /// `emote.<id>.name`.
    pub name: TextId,
    /// The player cannot emote again for this long.
    pub cooldown_ticks: u32,
    /// Icon name of the bubble; empty: the name is shown instead.
    pub icon: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct EmoteFile {
    #[serde(default = "default_cooldown")]
    pub cooldown_ticks: u32,
    /// Picture of the gesture (an icon of `assets/icons/`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::IconRef"))]
    pub icon: String,
}

fn default_cooldown() -> u32 {
    DEFAULT_EMOTE_COOLDOWN_TICKS
}

pub fn emote_name_key(id: &str) -> String {
    format!("emote.{id}.name")
}

pub(crate) fn parse_emote(id: &str, text: &str) -> Result<EmoteFile, String> {
    if !valid_id(id) {
        return Err(format!("emote id `{id}` must be [a-z0-9_]"));
    }
    let f: EmoteFile = ron::from_str(text).map_err(|e| format!("{id}.ron: {e}"))?;
    if f.cooldown_ticks == 0 {
        return Err("cooldown_ticks must be >= 1".into());
    }
    Ok(f)
}

/// Emotes, checked (`content/emotes/<id>.ron`), sorted by id.
pub(crate) fn validate_emotes(
    files: &BTreeMap<String, String>,
    keys: &i18n::KeyTable,
    mut err: impl FnMut(String, String),
) -> Vec<EmoteDef> {
    let mut out = Vec::with_capacity(files.len());
    for (id, text) in files {
        let path = format!("emotes/{id}.ron");
        let f = match parse_emote(id, text) {
            Ok(f) => f,
            Err(m) => {
                err(path, m);
                continue;
            }
        };
        let name = keys.id(&emote_name_key(id)).unwrap_or_else(|| {
            err(path.clone(), format!("no text key `{}`", emote_name_key(id)));
            TextId::new(0)
        });
        out.push(EmoteDef {
            id: id.clone(),
            name,
            cooldown_ticks: f.cooldown_ticks,
            icon: f.icon,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emote_needs_a_cooldown_and_a_clean_id() {
        assert!(parse_emote("wave", "()").is_ok());
        assert_eq!(parse_emote("wave", "()").map(|f| f.cooldown_ticks), Ok(40));
        assert!(parse_emote("wave", "(cooldown_ticks: 0)").is_err());
        assert!(parse_emote("Wave!", "()").is_err());
        assert!(parse_emote("wave", "(speed: 2)").is_err());
        let f = parse_emote("wave", r#"(icon: "emote_wave")"#).unwrap();
        assert_eq!(f.icon, "emote_wave");
    }
}
