//! The look files of `assets/` the "Floors and scenery" screen edits
//! ([pipeline de assets]):
//! `tiles.ron` (how each floor number is drawn) and `sprites/sprites.ron`
//! (anchor, animation strip and light of each folder or sprite).
//!
//! Same formats the client reads (`client/client/src/graphics.rs`). The
//! files are written in a fixed layout, one entry per line, so a change by
//! the editor reads well in a diff; comments inside them are not kept.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// Largest floor number (13 bits).
pub const MAX_FLOOR: u16 = 0x1fff;

/// A light, as the asset files write it.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LightDef {
    pub radius: f32,
    pub color: (f32, f32, f32),
    #[serde(default = "full")]
    pub strength: f32,
}

fn full() -> f32 {
    1.0
}

/// One floor number of `tiles.ron`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TileLook {
    #[serde(default)]
    pub sprite: String,
    #[serde(default)]
    pub wall: bool,
    #[serde(default)]
    pub light: Option<LightDef>,
    /// Drawn by day instead of `sprite`, unlit.
    #[serde(default)]
    pub day_sprite: Option<String>,
    /// Drawn instead of `sprite` while it rains.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rain_sprite: Option<String>,
    /// Drawn instead of `sprite` while it snows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snow_sprite: Option<String>,
}

/// How the images of a folder, or one sprite, are laid out (every field
/// optional: a sprite's own wins over its folder's).
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<(f32, f32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directions: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play: Option<Play>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Play {
    Always,
    Moving,
}

/// `sprites/sprites.ron`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default)]
    pub folders: BTreeMap<String, Layout>,
    #[serde(default)]
    pub sprites: BTreeMap<String, Layout>,
    #[serde(default)]
    pub lights: BTreeMap<String, LightDef>,
}

fn opts() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

pub fn read_tiles(assets: &Path) -> Result<BTreeMap<u16, TileLook>, String> {
    let text = std::fs::read_to_string(assets.join("tiles.ron")).unwrap_or_else(|_| "{}".into());
    opts().from_str(&text).map_err(|e| format!("tiles.ron: {e}"))
}

pub fn read_manifest(assets: &Path) -> Result<Manifest, String> {
    let path = assets.join("sprites").join("sprites.ron");
    let text = std::fs::read_to_string(path).unwrap_or_else(|_| "()".into());
    opts().from_str(&text).map_err(|e| format!("sprites.ron: {e}"))
}

fn name_ok(n: &str) -> bool {
    crate::art::valid_name(n)
}

fn check_light(at: &str, l: &LightDef, errs: &mut Vec<String>) {
    let unit = |v: f32| (0.0..=1.0).contains(&v);
    if !(l.radius > 0.0 && l.radius <= 32.0) {
        errs.push(format!("{at}: light radius must be above 0 and at most 32"));
    }
    if !(unit(l.color.0) && unit(l.color.1) && unit(l.color.2) && unit(l.strength)) {
        errs.push(format!("{at}: light color and strength go from 0 to 1"));
    }
}

/// Every problem of a `tiles.ron`; empty when it may be written.
pub fn check_tiles(tiles: &BTreeMap<u16, TileLook>) -> Vec<String> {
    let mut errs = Vec::new();
    for (floor, t) in tiles {
        let at = format!("floor {floor}");
        if *floor == 0 || *floor > MAX_FLOOR {
            errs.push(format!("{at}: floor numbers go from 1 to {MAX_FLOOR}"));
        }
        if !t.sprite.is_empty() && !name_ok(&t.sprite) {
            errs.push(format!("{at}: `{}` is not a sprite name", t.sprite));
        }
        if t.day_sprite.as_deref().is_some_and(|s| !name_ok(s)) {
            errs.push(format!("{at}: the day sprite is not a sprite name"));
        }
        if let Some(l) = &t.light {
            check_light(&at, l, &mut errs);
        }
    }
    errs
}

fn check_layout(at: &str, l: &Layout, errs: &mut Vec<String>) {
    if l.anchor
        .is_some_and(|(x, y)| !(x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0))
    {
        errs.push(format!("{at}: the anchor is a pixel of the image (0 or more)"));
    }
    if l.frames.is_some_and(|f| !(1..=64).contains(&f)) {
        errs.push(format!("{at}: frames go from 1 to 64"));
    }
    if l.fps.is_some_and(|f| !(f > 0.0 && f <= 60.0)) {
        errs.push(format!("{at}: frames per second go above 0 up to 60"));
    }
    if l.directions.is_some_and(|d| !matches!(d, 1 | 4)) {
        errs.push(format!("{at}: directions are 1 or 4"));
    }
}

/// Every problem of a `sprites.ron`; empty when it may be written.
pub fn check_manifest(m: &Manifest) -> Vec<String> {
    let mut errs = Vec::new();
    for (name, l) in &m.folders {
        if !crate::art::valid_folder(name) || name.is_empty() {
            errs.push(format!("folder `{name}`: not a folder name"));
        }
        check_layout(&format!("folder {name}"), l, &mut errs);
    }
    for (name, l) in &m.sprites {
        if !name_ok(name) {
            errs.push(format!("sprite `{name}`: not a sprite name"));
        }
        check_layout(&format!("sprite {name}"), l, &mut errs);
    }
    for (name, l) in &m.lights {
        if !name_ok(name) {
            errs.push(format!("light `{name}`: not a sprite name"));
        }
        check_light(&format!("light {name}"), l, &mut errs);
    }
    errs
}

fn num(v: f32) -> String {
    // `6` reads as an integer and the client wants a float: keep the dot.
    if v.fract() == 0.0 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

fn light_ron(l: &LightDef) -> String {
    let mut s = format!(
        "(radius: {}, color: ({}, {}, {})",
        num(l.radius),
        num(l.color.0),
        num(l.color.1),
        num(l.color.2)
    );
    if l.strength != 1.0 {
        let _ = write!(s, ", strength: {}", num(l.strength));
    }
    s.push(')');
    s
}

const TILES_HEAD: &str = "\
// How each floor number of the zone maps is drawn
// What blocks walking or sight is a
// game rule and stays in the zone map; this file is only the look. A floor
// with `light` shines at night and is drawn glowing; `day_sprite` replaces
// it by day, unlit. A brazier a player lights or puts out is two floors
// (map legend `switch`). Written by the editor.
";

pub fn tiles_ron(tiles: &BTreeMap<u16, TileLook>) -> String {
    let mut out = String::from(TILES_HEAD);
    out.push_str("{\n");
    for (floor, t) in tiles {
        let mut f = vec![format!("sprite: \"{}\"", t.sprite)];
        if t.wall {
            f.push("wall: true".into());
        }
        if let Some(l) = &t.light {
            f.push(format!("light: Some({})", light_ron(l)));
        }
        if let Some(d) = &t.day_sprite {
            f.push(format!("day_sprite: Some(\"{d}\")"));
        }
        let _ = writeln!(out, "    {floor}: ({}),", f.join(", "));
    }
    out.push_str("}\n");
    out
}

const MANIFEST_HEAD: &str = "\
// Anchors: the pixel of each image that sits on the center of its tile
// A sprite's name is its file name.
// An image may be a strip: `frames` side by side, and 1 or 4 `directions`
// as rows (S, W, N, E); `fps`, and `play` Always (scenery) or Moving (only
// while walking). A sprite's own fields win over its folder's. `lights`:
// sprites that shine at night. Written by the editor.
";

fn layout_ron(l: &Layout) -> String {
    let mut f = Vec::new();
    if let Some((x, y)) = l.anchor {
        f.push(format!("anchor: ({x}, {y})"));
    }
    if let Some(n) = l.frames {
        f.push(format!("frames: {n}"));
    }
    if let Some(n) = l.fps {
        f.push(format!(
            "fps: {}",
            if n.fract() == 0.0 { format!("{n}") } else { num(n) }
        ));
    }
    if let Some(n) = l.directions {
        f.push(format!("directions: {n}"));
    }
    if let Some(p) = l.play {
        f.push(format!("play: {p:?}"));
    }
    format!("({})", f.join(", "))
}

pub fn manifest_ron(m: &Manifest) -> String {
    let mut out = String::from(MANIFEST_HEAD);
    out.push_str("(\n    folders: {\n");
    for (k, l) in &m.folders {
        let _ = writeln!(out, "        \"{k}\": {},", layout_ron(l));
    }
    out.push_str("    },\n    sprites: {\n");
    for (k, l) in m.sprites.iter().filter(|(_, l)| **l != Layout::default()) {
        let _ = writeln!(out, "        \"{k}\": {},", layout_ron(l));
    }
    out.push_str("    },\n    lights: {\n");
    for (k, l) in &m.lights {
        let _ = writeln!(out, "        \"{k}\": {},", light_ron(l));
    }
    out.push_str("    },\n)\n");
    out
}

/// Writes through a temporary file, so the client never reads half a file.
pub fn write(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("ron.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn assets() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/assets")
    }

    #[test]
    fn the_real_files_round_trip() {
        let tiles = read_tiles(&assets()).unwrap();
        assert!(tiles.get(&6).is_some_and(|t| t.light.is_some()));
        assert_eq!(check_tiles(&tiles), Vec::<String>::new());
        let again: BTreeMap<u16, TileLook> = opts().from_str(&tiles_ron(&tiles)).unwrap();
        assert_eq!(again, tiles);

        let m = read_manifest(&assets()).unwrap();
        assert_eq!(m.folders.get("creatures").and_then(|l| l.directions), Some(4));
        assert_eq!(check_manifest(&m), Vec::<String>::new());
        let again: Manifest = opts().from_str(&manifest_ron(&m)).unwrap();
        assert_eq!(again, m);
    }

    #[test]
    fn bad_values_are_named() {
        let mut tiles = BTreeMap::new();
        tiles.insert(
            0,
            TileLook {
                sprite: "Bad Name".into(),
                wall: false,
                light: Some(LightDef {
                    radius: 0.0,
                    color: (2.0, 0.0, 0.0),
                    strength: 1.0,
                }),
                day_sprite: None,
                rain_sprite: None,
                snow_sprite: None,
            },
        );
        assert_eq!(check_tiles(&tiles).len(), 4);
        let mut m = Manifest::default();
        m.sprites.insert(
            "fire".into(),
            Layout {
                frames: Some(0),
                directions: Some(3),
                ..Layout::default()
            },
        );
        assert_eq!(check_manifest(&m).len(), 2);
    }
}
