//! Graphics as data (phase 6b):
//! every PNG under `assets/sprites/` and `assets/icons/` packed into one
//! sheet, named by file stem; `assets/tiles.ron` says how each floor number
//! is drawn; `assets/sprites/sprites.ron` gives anchors. Changed files are
//! reloaded while the game runs (development); `assets.pak` replaces the
//! loose files in a release (`tools/pack`).
//!
//! Phase 11: an image may be a strip of frames (one row per direction) that
//! plays at its own pace; `sprites.ron` says how many, per folder or sprite.
//!
//! Phase 10: light is data too. `tiles.ron` and `sprites.ron` (`lights`)
//! say which floors and sprites shine, and `assets/light.ron` gives the
//! color of the day hour by hour and the light a player carries.

use base::Direction;
use client_render::{Placed, Rgba, Sheet, Sprite};
use serde::Deserialize;
use serde::de::Error as _;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{info, warn};
use web_time::Instant;

const CHECK_EVERY: Duration = Duration::from_millis(700);

/// How one floor number is drawn.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileLook {
    /// Sprite name; empty or missing: the flat test color.
    #[serde(default)]
    pub sprite: String,
    /// Drawn standing (walls, trees): sorted with entities.
    #[serde(default)]
    pub wall: bool,
    /// Phase 10: the tile shines (a brazier) and is drawn glowing.
    #[serde(default)]
    pub light: Option<LightDef>,
    /// Alpha: drawn by day instead of `sprite`, without the glow and the
    /// light (a brazier put out at dawn).
    #[serde(default)]
    pub day_sprite: Option<String>,
}

/// Mean ambient level under which fires burn (full day is 1).
const NIGHT_BELOW: f32 = 0.92;

/// A light as the asset files write it.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightDef {
    /// Reach in tiles.
    pub radius: f32,
    /// Color, 0 to 1 each.
    pub color: (f32, f32, f32),
    #[serde(default = "full")]
    pub strength: f32,
}

fn full() -> f32 {
    1.0
}

impl LightDef {
    pub fn rgba(&self) -> Rgba {
        Rgba::new(self.color.0, self.color.1, self.color.2, self.strength)
    }
}

/// One ambient key: `(fraction of the day, (r, g, b))`.
pub type AmbientKey = (f32, (f32, f32, f32));

/// `assets/light.ron`: the color of the day and the carried light.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayLight {
    /// `(fraction of the day, color)`, sorted, from 0 (midnight) to 1.
    #[serde(deserialize_with = "sorted_keys")]
    pub ambient: Vec<AmbientKey>,
    /// Around the player's own character.
    pub carried: Option<LightDef>,
}

fn sorted_keys<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<AmbientKey>, D::Error> {
    let v: Vec<AmbientKey> = Vec::deserialize(d)?;
    if v.windows(2).any(|w| w[0].0 > w[1].0) {
        return Err(D::Error::custom("ambient keys must be sorted by time"));
    }
    Ok(v)
}

impl DayLight {
    /// The ambient color at this point of the day, between the two nearest
    /// keys (white without keys).
    pub fn ambient_at(&self, t: f64) -> Rgba {
        let t = t.rem_euclid(1.0) as f32;
        let (Some(first), Some(last)) = (self.ambient.first(), self.ambient.last()) else {
            return Rgba::WHITE;
        };
        let after = self.ambient.iter().position(|k| k.0 >= t);
        let (a, b) = match after {
            Some(0) | None => (last, first),
            Some(i) => (&self.ambient[i - 1], &self.ambient[i]),
        };
        let span = (b.0 - a.0).rem_euclid(1.0);
        let f = if span <= f32::EPSILON {
            0.0
        } else {
            ((t - a.0).rem_euclid(1.0) / span).clamp(0.0, 1.0)
        };
        let mix = |x: f32, y: f32| x + (y - x) * f;
        Rgba::new(mix(a.1.0, b.1.0), mix(a.1.1, b.1.1), mix(a.1.2, b.1.2), 1.0)
    }
}

/// When the frames of a strip play.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub enum Play {
    /// All the time (a fire, water).
    #[default]
    Always,
    /// Only while the being walks; standing shows the first frame.
    Moving,
}

/// How the images of a folder, or one sprite, are laid out and drawn.
/// Every field is optional: a sprite's own value wins over its folder's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Layout {
    /// The pixel of one frame that sits on the tile's center.
    #[serde(default)]
    anchor: Option<(f32, f32)>,
    /// Frames side by side in the strip (1: a still image).
    #[serde(default)]
    frames: Option<u32>,
    /// Frames per second.
    #[serde(default)]
    fps: Option<f32>,
    /// 1, or 4 rows: facing S, W, N, E (grid directions).
    #[serde(default)]
    directions: Option<u32>,
    #[serde(default)]
    play: Option<Play>,
}

impl Layout {
    fn over(self, base: Layout) -> Layout {
        Layout {
            anchor: self.anchor.or(base.anchor),
            frames: self.frames.or(base.frames),
            fps: self.fps.or(base.fps),
            directions: self.directions.or(base.directions),
            play: self.play.or(base.play),
        }
    }
}

/// Frames per second when the manifest does not say.
const FPS: f32 = 8.0;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    /// Layout of every image of a folder under `sprites/`.
    #[serde(default)]
    folders: BTreeMap<String, Layout>,
    /// Per sprite, over the folder's.
    #[serde(default)]
    sprites: BTreeMap<String, Layout>,
    /// Phase 10: sprites that shine (drawn glowing, with a light).
    #[serde(default)]
    lights: BTreeMap<String, LightDef>,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    /// The whole strip in the sheet.
    placed: Placed,
    anchor: (f32, f32),
    frames: u32,
    directions: u32,
    fps: f32,
    play: Play,
}

/// What a frame is drawn for: the time, and for a being where it faces and
/// whether it walks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Seconds on any steady clock; add a per-thing offset so equal things
    /// do not move in step.
    pub secs: f64,
    pub facing: Direction,
    pub moving: bool,
}

impl Pose {
    /// Scenery: plays all the time, faces the camera.
    pub fn still(secs: f64) -> Pose {
        Pose {
            secs,
            facing: Direction::S,
            moving: false,
        }
    }
}

/// Row of a four-direction strip: S, W, N, E. A diagonal takes the row of
/// the direction a quarter turn clockwise before it (SE draws as S).
fn row_of(d: Direction) -> u32 {
    match d {
        Direction::S | Direction::SE => 0,
        Direction::W | Direction::SW => 1,
        Direction::N | Direction::NW => 2,
        Direction::E | Direction::NE => 3,
    }
}

/// The login background, under the assets folder (made in the editor).
const BACKDROP: &str = "ui/login_background.png";
/// The game's icon (window and browser tab), under the assets folder
/// (made in the editor): a square PNG.
const GAME_ICON: &str = "ui/game_icon.png";
/// Wider than this only costs sheet space: the picture is drawn dimmed,
/// behind the login box.
const BACKDROP_WIDTH: u32 = 1280;

/// The image at most `max_w` wide, keeping its shape (box filter).
fn shrink(w: u32, h: u32, px: Vec<u8>, max_w: u32) -> (u32, u32, Vec<u8>) {
    if w <= max_w || w == 0 || h == 0 {
        return (w, h, px);
    }
    let nw = max_w;
    let nh = ((u64::from(h) * u64::from(nw)) / u64::from(w)).max(1) as u32;
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
            let mut sum = [0u32; 4];
            for sy in y0..y1.min(h) {
                for sx in x0..x1.min(w) {
                    let i = ((sy * w + sx) * 4) as usize;
                    for (c, v) in sum.iter_mut().zip(px.get(i..i + 4).unwrap_or(&[0; 4])) {
                        *c += u32::from(*v);
                    }
                }
            }
            let n = ((y1.min(h) - y0) * (x1.min(w) - x0)).max(1);
            let o = ((y * nw + x) * 4) as usize;
            if let Some(dst) = out.get_mut(o..o + 4) {
                for (d, c) in dst.iter_mut().zip(sum) {
                    *d = (c / n) as u8;
                }
            }
        }
    }
    (nw, nh, out)
}

/// One decoded PNG before packing.
struct Image {
    name: String,
    icon: bool,
    layout: Layout,
    w: u32,
    h: u32,
    px: Vec<u8>,
}

pub struct Graphics {
    dir: PathBuf,
    sheet: Sheet,
    sprites: BTreeMap<String, Entry>,
    icons: BTreeMap<String, Placed>,
    tiles: BTreeMap<u16, TileLook>,
    lights: BTreeMap<String, LightDef>,
    day: Option<DayLight>,
    /// The picture behind the login screens (`ui/login_background.png`).
    backdrop: Option<Placed>,
    /// The sheet changed and must go to the GPU.
    upload: bool,
    stamp: Option<SystemTime>,
    checked: Instant,
}

impl Graphics {
    /// Nothing loaded (before `new`, in tests).
    /// The game's icon (`ui/game_icon.png`) as RGBA8, if the game has one.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn game_icon(&self) -> Option<(u32, u32, Vec<u8>)> {
        game_icon(&self.dir)
    }

    /// The icon file itself, for the browser tab.
    #[cfg(target_arch = "wasm32")]
    pub fn game_icon_png(&self) -> Option<Vec<u8>> {
        client_files::read(self.dir.join(GAME_ICON))
            .ok()
            .filter(|b| !b.is_empty())
    }

    pub fn empty() -> Graphics {
        Graphics {
            dir: PathBuf::new(),
            sheet: Sheet::default(),
            sprites: BTreeMap::new(),
            icons: BTreeMap::new(),
            tiles: BTreeMap::new(),
            lights: BTreeMap::new(),
            day: None,
            backdrop: None,
            upload: false,
            stamp: None,
            checked: Instant::now(),
        }
    }

    pub fn new(dir: PathBuf) -> Graphics {
        let mut g = Graphics {
            dir,
            sheet: Sheet::default(),
            sprites: BTreeMap::new(),
            icons: BTreeMap::new(),
            tiles: BTreeMap::new(),
            lights: BTreeMap::new(),
            day: None,
            backdrop: None,
            upload: false,
            stamp: None,
            checked: Instant::now(),
        };
        g.load();
        g
    }

    fn load(&mut self) {
        self.stamp = newest(&self.dir);
        let manifest: Manifest = read_ron(&self.dir.join("sprites/sprites.ron")).unwrap_or_default();
        self.tiles = read_ron(&self.dir.join("tiles.ron")).unwrap_or_default();
        self.lights = manifest.lights.clone();
        self.day = read_ron(&self.dir.join("light.ron"));
        let mut images: Vec<Image> = Vec::new();
        for (path, folder) in pngs(&self.dir.join("sprites")) {
            let name = stem(&path);
            let base = manifest.folders.get(&folder).copied().unwrap_or_default();
            let layout = manifest
                .sprites
                .get(&name)
                .copied()
                .unwrap_or_default()
                .over(base);
            match decode(&path) {
                Ok((w, h, px)) => images.push(Image {
                    name,
                    icon: false,
                    layout,
                    w,
                    h,
                    px,
                }),
                Err(e) => warn!(path = %path.display(), "sprite not read: {e}"),
            }
        }
        for (path, _) in pngs(&self.dir.join("icons")) {
            match decode(&path) {
                Ok((w, h, px)) => images.push(Image {
                    name: stem(&path),
                    icon: true,
                    layout: Layout::default(),
                    w,
                    h,
                    px,
                }),
                Err(e) => warn!(path = %path.display(), "icon not read: {e}"),
            }
        }
        // The login background goes last, scaled down to what a screen needs.
        let backdrop = self.dir.join(BACKDROP);
        let has_backdrop = backdrop.is_file()
            && match decode(&backdrop) {
                Ok((w, h, px)) => {
                    let (w, h, px) = shrink(w, h, px, BACKDROP_WIDTH);
                    images.push(Image {
                        name: String::new(),
                        icon: false,
                        layout: Layout::default(),
                        w,
                        h,
                        px,
                    });
                    true
                }
                Err(e) => {
                    warn!(path = %backdrop.display(), "login background not read: {e}");
                    false
                }
            };
        let refs: Vec<(u32, u32, &[u8])> = images.iter().map(|i| (i.w, i.h, i.px.as_slice())).collect();
        self.sheet = client_render::pack(&refs);
        self.backdrop = None;
        if has_backdrop {
            images.pop();
            self.backdrop = self.sheet.placed.last().copied();
        }
        self.sprites.clear();
        self.icons.clear();
        for (img, placed) in images.iter().zip(&self.sheet.placed) {
            if img.icon {
                self.icons.insert(img.name.clone(), *placed);
            } else {
                self.sprites.insert(img.name.clone(), entry(img, *placed));
            }
        }
        self.upload = true;
        info!(
            sprites = self.sprites.len(),
            icons = self.icons.len(),
            sheet = %format!("{}x{}", self.sheet.width, self.sheet.height),
            "graphics loaded"
        );
    }

    /// Reloads when any file under the assets folder changed.
    pub fn reload_if_changed(&mut self) {
        if self.checked.elapsed() < CHECK_EVERY {
            return;
        }
        self.checked = Instant::now();
        if newest(&self.dir) != self.stamp {
            self.load();
        }
    }

    /// The next [`Graphics::take_upload`] returns the sheet again (the GPU
    /// copy was lost).
    pub fn request_upload(&mut self) {
        self.upload = true;
    }

    /// The sheet once after each (re)load, for the GPU.
    pub fn take_upload(&mut self) -> Option<&Sheet> {
        std::mem::take(&mut self.upload).then_some(&self.sheet)
    }

    /// The first frame, facing the camera.
    #[cfg(test)]
    pub fn sprite(&self, name: &str, tint: Rgba) -> Option<Sprite> {
        self.sprite_at(name, tint, Pose::still(0.0))
    }

    /// The frame of a strip for this moment, facing and walk.
    pub fn sprite_at(&self, name: &str, tint: Rgba, pose: Pose) -> Option<Sprite> {
        let e = self.sprites.get(name)?;
        let (fw, fh) = (e.placed.w / e.frames, e.placed.h / e.directions);
        let playing = e.frames > 1 && (e.play == Play::Always || pose.moving);
        let frame = if playing {
            ((pose.secs.max(0.0) * f64::from(e.fps)).floor() as u64 % u64::from(e.frames)) as u32
        } else {
            0
        };
        let row = if e.directions == 4 { row_of(pose.facing) } else { 0 };
        let cell = Placed {
            x: e.placed.x + frame * fw,
            y: e.placed.y + row * fh,
            w: fw,
            h: fh,
        };
        Some(Sprite {
            uv: self.sheet.uv(cell),
            size: (fw as f32, fh as f32),
            anchor: e.anchor,
            tint,
        })
    }

    /// The first frame of a sprite for each facing (S, W, N, E), for the
    /// interface: the creation preview and the character list.
    pub fn sheet_sprite(&self, name: &str) -> Option<client_ui::SheetSprite> {
        let e = self.sprites.get(name)?;
        let (fw, fh) = (e.placed.w / e.frames.max(1), e.placed.h / e.directions.max(1));
        let uv = std::array::from_fn(|row| {
            let row = if e.directions == 4 { row as u32 } else { 0 };
            self.sheet.uv(Placed {
                x: e.placed.x,
                y: e.placed.y + row * fh,
                w: fw,
                h: fh,
            })
        });
        Some(client_ui::SheetSprite {
            uv,
            aspect: fw as f32 / fh.max(1) as f32,
        })
    }

    /// The login background: sheet coordinates and width / height.
    pub fn backdrop(&self) -> Option<([f32; 4], f32)> {
        let p = self.backdrop?;
        Some((self.sheet.uv(p), p.w as f32 / p.h.max(1) as f32))
    }

    /// Sheet coordinates of an icon (`u` shifted by 2), or of the generic
    /// icon `fallback` when `name` has no image.
    pub fn icon_uv(&self, name: Option<&str>, fallback: &str) -> Option<[f32; 4]> {
        let placed = name
            .and_then(|n| self.icons.get(n))
            .or_else(|| self.icons.get(fallback))?;
        Some(self.sheet.uv(*placed))
    }

    pub fn tile(&self, floor: u16) -> Option<&TileLook> {
        self.tiles.get(&floor)
    }

    /// Phase 10: the light of a sprite that shines.
    pub fn sprite_light(&self, name: &str) -> Option<LightDef> {
        self.lights.get(name).copied()
    }

    /// Phase 10: the ambient color at this point of the day (`None`: the
    /// server has not said the time yet, full daylight).
    pub fn ambient(&self, day_fraction: Option<f64>) -> Rgba {
        match (&self.day, day_fraction) {
            (Some(d), Some(t)) => d.ambient_at(t),
            _ => Rgba::WHITE,
        }
    }

    /// Night, dusk or dawn: the hours when fires burn. Before the server
    /// says the time it is day.
    pub fn is_night(&self, day_fraction: Option<f64>) -> bool {
        let a = self.ambient(day_fraction);
        (a.r + a.g + a.b) / 3.0 < NIGHT_BELOW
    }

    pub fn carried_light(&self) -> Option<LightDef> {
        self.day.as_ref().and_then(|d| d.carried)
    }
}

/// A packed image with its layout checked: a strip that does not divide
/// into whole frames and rows is drawn whole, with a warning.
fn entry(img: &Image, placed: Placed) -> Entry {
    let l = img.layout;
    let mut frames = l.frames.unwrap_or(1).max(1);
    let mut directions = l.directions.unwrap_or(1).max(1);
    if !matches!(directions, 1 | 4) || !img.w.is_multiple_of(frames) || !img.h.is_multiple_of(directions) {
        warn!(
            sprite = %img.name,
            "{}x{} does not split into {frames} frames by {directions} directions (1 or 4): drawn whole",
            img.w, img.h
        );
        frames = 1;
        directions = 1;
    }
    Entry {
        placed,
        anchor: l.anchor.unwrap_or((0.0, 0.0)),
        frames,
        directions,
        fps: l.fps.filter(|f| *f > 0.0).unwrap_or(FPS),
        play: l.play.unwrap_or_default(),
    }
}

/// RON with `Some(..)` optional: `anchor: (32, 58)` reads as set, the way
/// the manifest has always been written.
fn read_ron<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = client_files::read_to_string(path).ok()?;
    let opts = ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    match opts.from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            warn!(path = %path.display(), "{e}");
            None
        }
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Every PNG under `dir`, with the name of its first folder below `dir`.
fn pngs(dir: &Path) -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, folder: &str, out: &mut Vec<(PathBuf, String)>) {
        let Ok(mut entries) = client_files::list(dir) else {
            return;
        };
        entries.sort();
        for client_files::Entry { path: p, is_dir } in entries {
            if is_dir {
                let f = if folder.is_empty() {
                    stem(&p)
                } else {
                    folder.to_owned()
                };
                walk(&p, &f, out);
            } else if p.extension().is_some_and(|e| e == "png") {
                out.push((p, folder.to_owned()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out);
    out
}

/// Latest modification time of anything under `dir`.
fn newest(dir: &Path) -> Option<SystemTime> {
    let mut best = client_files::modified(dir);
    if let Ok(entries) = client_files::list(dir) {
        for e in entries {
            let t = if e.is_dir {
                newest(&e.path)
            } else {
                client_files::modified(&e.path)
            };
            best = best.max(t);
        }
    }
    best
}

/// The game's icon as RGBA8, when the game has one.
#[cfg(not(target_arch = "wasm32"))]
fn game_icon(assets: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let path = assets.join(GAME_ICON);
    if !client_files::read(&path).is_ok_and(|b| !b.is_empty()) {
        return None;
    }
    decode(&path)
        .map_err(|e| warn!(path = %path.display(), "game icon not read: {e}"))
        .ok()
}

/// PNG to RGBA8.
fn decode(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let bytes = client_files::read(path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let size = reader.output_buffer_size().ok_or("image too large")?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let px = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        other => return Err(format!("unsupported color type {other:?}")),
    };
    Ok((info.width, info.height, px))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> Graphics {
        Graphics::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/assets"))
    }

    #[test]
    fn the_real_assets_load() {
        let mut g = assets();
        assert!(g.take_upload().is_some());
        assert!(g.take_upload().is_none());
        let wolf = g.sprite("wolf", Rgba::WHITE).expect("wolf sprite");
        assert_eq!(wolf.size, (64.0, 64.0));
        assert!(wolf.anchor.1 > 40.0, "creatures stand on their feet");
        assert!(g.icon_uv(Some("glow_herb"), "generic_item").is_some());
        let (w, h, px) = shrink(4, 2, vec![255; 4 * 2 * 4], 2);
        assert_eq!((w, h, px.len()), (2, 1, 8));
        assert_eq!(
            g.icon_uv(Some("nope"), "generic_item"),
            g.icon_uv(None, "generic_item")
        );
        assert!(g.tile(3).is_some_and(|t| t.wall));
        assert!(g.tile(6).is_some_and(|t| t.light.is_some()), "braziers shine");
        assert!(g.carried_light().is_some());
    }

    /// A 4-frame, 4-direction strip of 32×16 frames at (100, 200) in a
    /// 1000×1000 sheet.
    fn strip(play: Play) -> Graphics {
        let mut g = Graphics::empty();
        g.sheet.width = 1000;
        g.sheet.height = 1000;
        let img = Image {
            name: "walker".into(),
            icon: false,
            layout: Layout {
                anchor: Some((16.0, 14.0)),
                frames: Some(4),
                fps: Some(2.0),
                directions: Some(4),
                play: Some(play),
            },
            w: 128,
            h: 64,
            px: Vec::new(),
        };
        let placed = Placed {
            x: 100,
            y: 200,
            w: 128,
            h: 64,
        };
        g.sprites.insert("walker".into(), entry(&img, placed));
        g
    }

    fn cell(g: &Graphics, pose: Pose) -> (f32, f32) {
        let s = g.sprite_at("walker", Rgba::WHITE, pose).expect("sprite");
        assert_eq!(s.size, (32.0, 16.0), "one frame");
        assert_eq!(s.anchor, (16.0, 14.0));
        ((s.uv[0] - 2.0) * 1000.0, s.uv[1] * 1000.0)
    }

    #[test]
    fn a_strip_plays_its_frames_and_faces_its_way() {
        let g = strip(Play::Moving);
        let walk = |secs, facing| Pose {
            secs,
            facing,
            moving: true,
        };
        let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01;
        assert!(near(cell(&g, walk(0.0, Direction::S)), (100.0, 200.0)));
        // 2 frames a second: at 1.6 s the fourth frame (x + 3 × 32).
        assert!(near(cell(&g, walk(1.6, Direction::S)), (196.0, 200.0)));
        // And round again.
        assert!(near(cell(&g, walk(2.1, Direction::S)), (100.0, 200.0)));
        // Rows: S, W, N, E; a diagonal takes the row before it.
        assert!(near(cell(&g, walk(0.0, Direction::N)), (100.0, 232.0)));
        assert!(near(cell(&g, walk(0.0, Direction::NE)), (100.0, 248.0)));
        assert!(near(cell(&g, walk(0.0, Direction::SW)), (100.0, 216.0)));
        // Standing still: the first frame.
        let stand = Pose {
            secs: 1.6,
            facing: Direction::W,
            moving: false,
        };
        assert!(near(cell(&g, stand), (100.0, 216.0)));
        // Scenery plays standing still.
        let fire = strip(Play::Always);
        assert!(near(cell(&fire, Pose::still(1.6)), (196.0, 200.0)));
    }

    #[test]
    fn a_strip_that_does_not_divide_is_drawn_whole() {
        let img = Image {
            name: "odd".into(),
            icon: false,
            layout: Layout {
                frames: Some(3),
                directions: Some(2),
                ..Layout::default()
            },
            w: 100,
            h: 64,
            px: Vec::new(),
        };
        let e = entry(
            &img,
            Placed {
                x: 0,
                y: 0,
                w: 100,
                h: 64,
            },
        );
        assert_eq!((e.frames, e.directions), (1, 1));
    }

    #[test]
    fn the_day_turns_dark_at_night() {
        let g = assets();
        let noon = g.ambient(Some(0.5));
        let midnight = g.ambient(Some(0.0));
        assert!(noon.r > 0.95 && noon.g > 0.95);
        assert!(midnight.r < 0.6 && midnight.b > midnight.r, "{midnight:?}");
        // Wraps around midnight without a jump.
        let before = g.ambient(Some(0.999));
        assert!((before.r - midnight.r).abs() < 0.05);
        assert_eq!(g.ambient(None), Rgba::WHITE, "no time yet: daylight");
        assert!(g.is_night(Some(0.0)) && !g.is_night(Some(0.5)) && !g.is_night(None));
        // A lit brazier shines at night and shows unlit by day;
        // players also put it out: that floor has no light.
        assert!(
            g.tile(6)
                .is_some_and(|t| t.day_sprite.as_deref() == Some("brazier_unlit") && t.light.is_some())
        );
        assert!(g.tile(19).is_some_and(|t| t.light.is_none()));
    }
}
