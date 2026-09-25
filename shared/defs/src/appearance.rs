//! Character creation choices (`content/appearance/appearance.ron`, phase 5,
//!): body types, skin tones, hairstyles and hair
//! colors. New options are content: append, never remove, so the
//! text ids saved in sheets keep meaning the same thing.

use crate::npc::valid_id;
use base::{Look, TextId};
use i18n::KeyTable;
use serde::{Deserialize, Serialize};

/// Most options per list: positions travel as `u8`.
pub const MAX_LOOK_OPTIONS: usize = 64;

/// The four lists, in `Look` field order.
pub const LOOK_LISTS: [&str; 4] = ["body", "skin", "hair_style", "hair_color"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookOption {
    pub id: String,
    /// `appearance.<list>.<id>`.
    pub name: TextId,
    /// `0xRRGGBB`: the tone for the palette swap (skin, hair color); the
    /// placeholder figure until the sprites of phase 6 also uses it for bodies
    /// and hairstyles.
    pub color: u32,
}

/// Lists in file order; a `Look` holds positions in them. Empty when the
/// content has no `appearance/appearance.ron`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppearanceDef {
    pub lists: [Vec<LookOption>; 4],
}

impl AppearanceDef {
    pub fn bodies(&self) -> &[LookOption] {
        &self.lists[0]
    }

    pub fn skins(&self) -> &[LookOption] {
        &self.lists[1]
    }

    pub fn hair_styles(&self) -> &[LookOption] {
        &self.lists[2]
    }

    pub fn hair_colors(&self) -> &[LookOption] {
        &self.lists[3]
    }

    /// Positions of the saved text ids; unknown or empty ids (sheets from
    /// before phase 5) become the first option.
    pub fn look_of(&self, ids: [&str; 4]) -> Look {
        let pos = |list: &[LookOption], id: &str| -> u8 {
            list.iter()
                .position(|o| o.id == id)
                .and_then(|p| u8::try_from(p).ok())
                .unwrap_or(0)
        };
        Look {
            body: pos(self.bodies(), ids[0]),
            skin: pos(self.skins(), ids[1]),
            hair_style: pos(self.hair_styles(), ids[2]),
            hair_color: pos(self.hair_colors(), ids[3]),
        }
    }

    /// Text ids of a look chosen on the client; `None` if a position is
    /// past the end of its list.
    pub fn ids_of(&self, look: Look) -> Option<[String; 4]> {
        let id = |list: &[LookOption], p: u8| list.get(usize::from(p)).map(|o| o.id.clone());
        Some([
            id(self.bodies(), look.body)?,
            id(self.skins(), look.skin)?,
            id(self.hair_styles(), look.hair_style)?,
            id(self.hair_colors(), look.hair_color)?,
        ])
    }
}

/// Text key of an option's name.
pub fn look_name_key(list: &str, id: &str) -> String {
    format!("appearance.{list}.{id}")
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct OptionFile {
    id: String,
    /// `#rrggbb`; empty: gray.
    #[serde(default)]
    color: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct AppearanceFile {
    bodies: Vec<OptionFile>,
    skins: Vec<OptionFile>,
    hair_styles: Vec<OptionFile>,
    hair_colors: Vec<OptionFile>,
}

fn parse_color(s: &str) -> Option<u32> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

/// Parses and checks the file: every list has 1 to `MAX_LOOK_OPTIONS`
/// options with unique ids, a name key and, if given, a `#rrggbb` color.
pub(crate) fn parse_appearance(text: &str, keys: &KeyTable) -> Result<AppearanceDef, Vec<String>> {
    let file: AppearanceFile = ron::from_str(text).map_err(|e| vec![e.to_string()])?;
    let mut problems = Vec::new();
    let mut def = AppearanceDef::default();
    let lists = [file.bodies, file.skins, file.hair_styles, file.hair_colors];
    for ((name, raw), out) in LOOK_LISTS.iter().zip(lists).zip(def.lists.iter_mut()) {
        if raw.is_empty() || raw.len() > MAX_LOOK_OPTIONS {
            problems.push(format!("{name}: 1 to {MAX_LOOK_OPTIONS} options"));
        }
        for o in raw {
            if !valid_id(&o.id) {
                problems.push(format!("{name}: invalid id `{}`", o.id));
                continue;
            }
            if out.iter().any(|x| x.id == o.id) {
                problems.push(format!("{name}: duplicate id `{}`", o.id));
                continue;
            }
            let color = match o.color.as_str() {
                "" => 0x808080,
                c => parse_color(c).unwrap_or_else(|| {
                    problems.push(format!("{name}.{}: color `{c}` is not #rrggbb", o.id));
                    0
                }),
            };
            let key = look_name_key(name, &o.id);
            let Some(text) = keys.id(&key) else {
                problems.push(format!("{name}.{}: no text key `{key}`", o.id));
                continue;
            };
            out.push(LookOption {
                id: o.id,
                name: text,
                color,
            });
        }
    }
    if problems.is_empty() {
        Ok(def)
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> KeyTable {
        let text = r#"{
            "appearance.body.a": "A", "appearance.skin.s": "S",
            "appearance.hair_style.h": "H", "appearance.hair_style.bald": "B",
            "appearance.hair_color.c": "C",
        }"#;
        KeyTable::from_source(&[("appearance.ron", text)]).unwrap()
    }

    #[test]
    fn parses_and_maps_ids() {
        let text = r##"(
            bodies: [(id: "a")],
            skins: [(id: "s", color: "#c08060")],
            hair_styles: [(id: "h"), (id: "bald")],
            hair_colors: [(id: "c", color: "#101010")],
        )"##;
        let def = parse_appearance(text, &keys()).unwrap();
        assert_eq!(def.skins()[0].color, 0xc08060);
        let look = def.look_of(["a", "s", "bald", "c"]);
        assert_eq!(look.hair_style, 1);
        assert_eq!(def.look_of(["", "", "", ""]), Look::default());
        assert_eq!(def.ids_of(look).unwrap()[2], "bald");
        assert!(def.ids_of(Look { body: 3, ..look }).is_none());
    }

    #[test]
    fn reports_problems() {
        let text = r##"(
            bodies: [],
            skins: [(id: "s", color: "red")],
            hair_styles: [(id: "h"), (id: "h")],
            hair_colors: [(id: "missing")],
        )"##;
        let p = parse_appearance(text, &keys()).unwrap_err();
        assert_eq!(p.len(), 4, "{p:?}");
    }
}
