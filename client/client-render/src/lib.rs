//! Draws the isometric world: projection, camera, depth order.
//! Knows nothing of `WorldState`, `protocol` nor the window library: it
//! draws a list of items on a `wgpu` surface it is given.
//!
//! Phase 1:
//! flat test shapes in one batch of colored triangles. Phase 5: the
//! interface (`UiVertex`) and its glyph atlas in the same batch. Phase 6:
//! sprites from a packed RGBA sheet (`sheet`, asset pipeline); light
//! shaders in phase 10.

mod digits;
mod gpu;
mod sheet;

pub use sheet::{Placed, Sheet, pack};

use base::TilePos;
pub use gpu::{RenderError, Screenshot};

/// Linear-free RGBA color, components in 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Rgba { r, g, b, a }
    }

    pub const WHITE: Rgba = Rgba::new(1.0, 1.0, 1.0, 1.0);

    fn shade(self, k: f32) -> Rgba {
        Rgba::new(self.r * k, self.g * k, self.b * k, self.a)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderConfig {
    /// Tile width and height on screen at zoom 1 (`L` and `A` in the docs).
    pub tile_w: f32,
    pub tile_h: f32,
    /// Height of a wall block, in pixels at zoom 1.
    pub wall_h: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            tile_w: 64.0,
            tile_h: 32.0,
            wall_h: 40.0,
        }
    }
}

/// What the screen shows: `center` (in tiles) lands on the middle of a
/// screen of `screen` pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub center: (f32, f32),
    pub zoom: f32,
    pub screen: (f32, f32),
}

impl Camera {
    /// Top-left of the screen in world pixels (`camera_x`, `camera_y`).
    fn origin(&self, cfg: &RenderConfig) -> (f32, f32) {
        let (wx, wy) = world_px(cfg, self.center.0, self.center.1);
        (
            wx * self.zoom - self.screen.0 / 2.0,
            wy * self.zoom - self.screen.1 / 2.0,
        )
    }
}

fn world_px(cfg: &RenderConfig, tx: f32, ty: f32) -> (f32, f32) {
    ((tx - ty) * cfg.tile_w / 2.0, (tx + ty) * cfg.tile_h / 2.0)
}

/// Screen position of the center of tile `(tx, ty)` (fractional allowed).
pub fn tile_to_screen(cfg: &RenderConfig, cam: &Camera, tx: f32, ty: f32) -> (f32, f32) {
    let (wx, wy) = world_px(cfg, tx, ty);
    let (cx, cy) = cam.origin(cfg);
    (wx * cam.zoom - cx, wy * cam.zoom - cy)
}

/// Tile under a screen point (inverse of `tile_to_screen`, rounded).
pub fn screen_to_tile(cfg: &RenderConfig, cam: &Camera, x: f32, y: f32) -> TilePos {
    let (cx, cy) = cam.origin(cfg);
    let (wx, wy) = ((x + cx) / cam.zoom, (y + cy) / cam.zoom);
    let a = wx / (cfg.tile_w / 2.0);
    let b = wy / (cfg.tile_h / 2.0);
    TilePos::new(((a + b) / 2.0).round() as i32, ((b - a) / 2.0).round() as i32)
}

/// Tiles whose center may be on screen, with a margin of `margin` tiles.
pub fn visible_tiles(cfg: &RenderConfig, cam: &Camera, margin: i32) -> (TilePos, TilePos) {
    let corners = [
        (0.0, 0.0),
        (cam.screen.0, 0.0),
        (0.0, cam.screen.1),
        (cam.screen.0, cam.screen.1),
    ]
    .map(|(x, y)| screen_to_tile(cfg, cam, x, y));
    let min_x = corners.iter().map(|p| p.x).min().unwrap_or(0) - margin;
    let max_x = corners.iter().map(|p| p.x).max().unwrap_or(0) + margin;
    let min_y = corners.iter().map(|p| p.y).min().unwrap_or(0) - margin;
    let max_y = corners.iter().map(|p| p.y).max().unwrap_or(0) + margin;
    (TilePos::new(min_x, min_y), TilePos::new(max_x, max_y))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Floor,
    Object,
    Effect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// Flat diamond.
    Floor(Rgba),
    /// Raised block.
    Wall(Rgba),
    /// Standing figure.
    Figure(Rgba),
    /// Outline of a tile (cursor, destination).
    Marker(Rgba),
    /// An image of the sprite sheet, its anchor on the tile's ground point.
    Sprite(Sprite),
    /// Phase 10: the same, glowing: drawn at full brightness whatever the
    /// light (a brazier, a glowing herb), usually beside a [`PointLight`].
    Glow(Sprite),
}

/// Phase 10: a light on the ground, in tile coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    pub tile: (f32, f32),
    /// Reach in tiles.
    pub radius: f32,
    /// Color; `a` is the strength.
    pub color: Rgba,
}

/// Phase 10: the world's light for the next frames: the ambient color of
/// the time of day and the point lights in view.
#[derive(Clone, Debug, PartialEq)]
pub struct Lighting {
    pub ambient: Rgba,
    pub lights: Vec<PointLight>,
}

impl Default for Lighting {
    /// Full daylight, no lights: the world as drawn before phase 10.
    fn default() -> Lighting {
        Lighting {
            ambient: Rgba::WHITE,
            lights: Vec::new(),
        }
    }
}

/// The ground in the player's sight, one byte per tile (255 in
/// sight, 0 not; values between soften the edge). Ground outside the mask
/// counts as not seen. The shader blends between tile centers, so the edge
/// is a gradient, not tile steps.
#[derive(Clone, Debug, PartialEq)]
pub struct SightMask {
    /// Tile of the first byte (top-left of the square).
    pub origin: TilePos,
    pub side: u32,
    pub seen: Vec<u8>,
    /// What unseen ground is multiplied by.
    pub shade: Rgba,
}

/// One image of the sheet as drawn in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    /// From [`Sheet::uv`].
    pub uv: [f32; 4],
    /// Size in pixels at zoom 1.
    pub size: (f32, f32),
    /// The pixel of the image that sits on the tile's center, from its top left.
    pub anchor: (f32, f32),
    /// Multiplies the image colors (white: unchanged).
    pub tint: Rgba,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemDesign {
    pub shape: Shape,
    pub tile: (f32, f32),
    pub layer: Layer,
}

/// Debug numbers drawn on top, in screen pixels. Only digits, `-`, `,`,
/// `.` and spaces (no font before phase 5).
#[derive(Clone, Debug, PartialEq)]
pub struct HudDigits {
    pub text: String,
    pub pos: (f32, f32),
    pub height: f32,
    pub color: Rgba,
}

/// Draw order: layer, then depth `x + y`, then `x` (stable for equal depth).
pub fn sort_items(items: &mut [ItemDesign]) {
    items.sort_by(|a, b| {
        let key = |i: &ItemDesign| (i.layer, i.tile.0 + i.tile.1, i.tile.0);
        let (ka, kb) = (key(a), key(b));
        ka.0.cmp(&kb.0)
            .then(ka.1.total_cmp(&kb.1))
            .then(ka.2.total_cmp(&kb.2))
    });
}

/// Triangles in screen pixels, built on the CPU each frame.
#[derive(Default)]
pub(crate) struct Batch {
    pub(crate) verts: Vec<gpu::Vertex>,
    /// 1 while drawing the lit world, 0 for glowing shapes and the interface.
    pub(crate) lit: f32,
}

impl Batch {
    fn tri(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32), color: Rgba) {
        let col = [color.r, color.g, color.b, color.a];
        for p in [a, b, c] {
            self.verts.push(gpu::Vertex {
                pos: [p.0, p.1],
                uv: gpu::FLAT,
                color: col,
                lit: self.lit,
            });
        }
    }

    /// A textured rectangle `(x0, y0, x1, y1)` with sheet coordinates `uv`.
    fn image(&mut self, (x0, y0, x1, y1): (f32, f32, f32, f32), uv: [f32; 4], c: Rgba) {
        let col = [c.r, c.g, c.b, c.a];
        let lit = self.lit;
        let v = |x: f32, y: f32, u: f32, w: f32| gpu::Vertex {
            pos: [x, y],
            uv: [u, w],
            color: col,
            lit,
        };
        let [u0, v0, u1, v1] = uv;
        self.verts.extend([
            v(x0, y0, u0, v0),
            v(x1, y0, u1, v0),
            v(x1, y1, u1, v1),
            v(x0, y0, u0, v0),
            v(x1, y1, u1, v1),
            v(x0, y1, u0, v1),
        ]);
    }

    fn quad(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32), color: Rgba) {
        self.tri(a, b, c, color);
        self.tri(a, c, d, color);
    }

    pub(crate) fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        self.quad((x, y), (x + w, y), (x + w, y + h), (x, y + h), color);
    }

    fn diamond(&mut self, x: f32, y: f32, hw: f32, hh: f32, c: Rgba) {
        self.quad((x, y - hh), (x + hw, y), (x, y + hh), (x - hw, y), c);
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: Rgba) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        self.quad(
            (a.0 + nx, a.1 + ny),
            (b.0 + nx, b.1 + ny),
            (b.0 - nx, b.1 - ny),
            (a.0 - nx, a.1 - ny),
            c,
        );
    }

    fn circle(&mut self, x: f32, y: f32, r: f32, c: Rgba) {
        const SIDES: usize = 16;
        let point = |i: usize| {
            let t = i as f32 / SIDES as f32 * std::f32::consts::TAU;
            (x + r * t.cos(), y + r * t.sin())
        };
        for i in 0..SIDES {
            self.tri((x, y), point(i), point(i + 1), c);
        }
    }

    fn item(&mut self, cfg: &RenderConfig, cam: &Camera, item: &ItemDesign) {
        let hw = cfg.tile_w / 2.0 * cam.zoom;
        let hh = cfg.tile_h / 2.0 * cam.zoom;
        let (x, y) = tile_to_screen(cfg, cam, item.tile.0, item.tile.1);
        let wall = cfg.wall_h * cam.zoom;
        if x < -2.0 * hw || y < -2.0 * hh - wall || x > cam.screen.0 + 2.0 * hw || y > cam.screen.1 + 2.0 * hh
        {
            return;
        }
        // The ground (floor layer) also takes the sight mask.
        self.lit = match item.shape {
            Shape::Glow(_) | Shape::Marker(_) => 0.0,
            _ if item.layer == Layer::Floor => 2.0,
            _ => 1.0,
        };
        match item.shape {
            Shape::Floor(c) => self.diamond(x, y, hw, hh, c),
            Shape::Wall(c) => {
                let (l, r, b) = ((x - hw, y), (x + hw, y), (x, y + hh));
                let up = |p: (f32, f32)| (p.0, p.1 - wall);
                self.quad(up(l), up(b), b, l, c.shade(0.7));
                self.quad(up(b), up(r), r, b, c.shade(0.85));
                self.diamond(x, y - wall, hw, hh, c);
            }
            Shape::Figure(c) => {
                let r = hh * 0.6;
                self.circle(x, y, r, c.shade(0.4));
                self.circle(x, y - 2.0 * r, r * 1.1, c);
            }
            Shape::Sprite(s) | Shape::Glow(s) => {
                let z = cam.zoom;
                let (x0, y0) = (x - s.anchor.0 * z, y - s.anchor.1 * z);
                let (x1, y1) = (x0 + s.size.0 * z, y0 + s.size.1 * z);
                self.image((x0, y0, x1, y1), s.uv, s.tint);
            }
            Shape::Marker(c) => {
                let p = [(x, y - hh), (x + hw, y), (x, y + hh), (x - hw, y)];
                for i in 0..4 {
                    self.line(p[i], p[(i + 1) % 4], 2.0, c);
                }
            }
        }
    }
}

/// A screen-space vertex of the interface (`client-ui` output), drawn over
/// the world. `uv` in 0..=1 samples the glyph atlas; negative is a flat
/// color; `u >= 2` samples the sprite sheet (icons, [`Sheet::uv`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// A new glyph atlas: `size` × `size` coverage bytes.
pub struct Atlas<'a> {
    pub size: u32,
    pub pixels: &'a [u8],
}

pub struct Render {
    cfg: RenderConfig,
    gpu: gpu::Gpu,
    batch: Batch,
    lighting: Lighting,
    sight: Option<SightMask>,
    /// The mask changed since the last upload.
    sight_dirty: bool,
}

impl Render {
    /// `target` is the window (anything `wgpu` can draw on); `size` its
    /// size in physical pixels.
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        size: (u32, u32),
        cfg: RenderConfig,
    ) -> Result<Self, RenderError> {
        Ok(Render {
            cfg,
            gpu: gpu::Gpu::new(target, size).await?,
            batch: Batch::default(),
            lighting: Lighting::default(),
            sight: None,
            sight_dirty: false,
        })
    }

    pub fn config(&self) -> &RenderConfig {
        &self.cfg
    }

    pub fn resize(&mut self, size: (u32, u32)) {
        self.gpu.resize(size);
    }

    /// The graphics device was lost. Drop this `Render`, build another with
    /// [`Render::new`] and send the atlas and the sprite sheet again.
    /// Asks for a copy of the next drawn frame; false when this window's
    /// surface cannot be copied.
    pub fn request_screenshot(&mut self) -> bool {
        self.gpu.request_screenshot()
    }

    /// The frame copied since the last call, if any.
    pub fn take_screenshot(&mut self) -> Option<Screenshot> {
        self.gpu.take_screenshot()
    }

    pub fn is_lost(&self) -> bool {
        self.gpu.is_lost()
    }

    pub fn screen_to_tile(&self, cam: &Camera, x: f32, y: f32) -> TilePos {
        screen_to_tile(&self.cfg, cam, x, y)
    }

    /// Phase 10: the light of the next frames (time of day, lights in view).
    pub fn set_lighting(&mut self, lighting: Lighting) {
        self.lighting = lighting;
    }

    /// The ground in sight (`None`: everything is drawn as seen).
    pub fn set_sight(&mut self, sight: Option<SightMask>) {
        if self.sight != sight {
            self.sight = sight;
            self.sight_dirty = true;
        }
    }

    /// Sorts and draws one frame: the world, debug digits, then the
    /// interface on top. Leaves `items` sorted (reuse the buffer).
    pub fn draw(
        &mut self,
        cam: &Camera,
        items: &mut [ItemDesign],
        hud: &[HudDigits],
        ui: &[UiVertex],
        atlas: Option<Atlas<'_>>,
        sheet: Option<&Sheet>,
    ) {
        if let Some(a) = atlas {
            self.gpu.upload_atlas(a.size, a.pixels);
        }
        if let Some(s) = sheet {
            self.gpu.upload_sheet(s.width, s.height, &s.pixels);
        }
        if std::mem::take(&mut self.sight_dirty)
            && let Some(m) = &self.sight
        {
            self.gpu.upload_fog(m.side, &m.seen);
        }
        sort_items(items);
        self.batch.verts.clear();
        for item in items.iter() {
            self.batch.item(&self.cfg, cam, item);
        }
        self.batch.lit = 0.0;
        for h in hud {
            digits::draw(&mut self.batch, &h.text, h.pos, h.height, h.color);
        }
        self.batch.verts.extend(ui.iter().map(|v| gpu::Vertex {
            pos: v.pos,
            uv: v.uv,
            color: v.color,
            lit: 0.0,
        }));
        let light = uniforms(&self.cfg, cam, &self.lighting, self.sight.as_ref());
        self.gpu.present(&self.batch.verts, &light);
    }
}

/// The light in shader form: lights in screen pixels, the nearest to the
/// screen's center first when there are more than the shader takes.
pub(crate) fn uniforms(
    cfg: &RenderConfig,
    cam: &Camera,
    l: &Lighting,
    sight: Option<&SightMask>,
) -> gpu::Uniforms {
    let (ox, oy) = cam.origin(cfg);
    let (fog_box, fog_shade) = match sight {
        Some(m) if m.side > 0 && m.seen.len() == (m.side as usize).pow(2) => (
            [m.origin.x as f32, m.origin.y as f32, m.side as f32, 1.0],
            [m.shade.r, m.shade.g, m.shade.b, 1.0],
        ),
        _ => ([0.0; 4], [1.0; 4]),
    };
    let mut u = gpu::Uniforms {
        fog_map: [ox, oy, cfg.tile_w / 2.0 * cam.zoom, cfg.tile_h / 2.0 * cam.zoom],
        fog_box,
        fog_shade,
        size: [cam.screen.0, cam.screen.1],
        count: 0.0,
        _pad: 0.0,
        ambient: [l.ambient.r, l.ambient.g, l.ambient.b, 1.0],
        lights: [[0.0; 4]; gpu::MAX_LIGHTS],
        colors: [[0.0; 4]; gpu::MAX_LIGHTS],
    };
    let (cx, cy) = (cam.screen.0 / 2.0, cam.screen.1 / 2.0);
    // One tile east is half a tile width right and half a height down: on
    // the shader's ellipse (y doubled) that is `tile_w / 2 · √2` pixels.
    let per_tile = cfg.tile_w / 2.0 * cam.zoom * std::f32::consts::SQRT_2;
    let mut near: Vec<(f32, [f32; 4], [f32; 4])> = l
        .lights
        .iter()
        .map(|p| {
            let (x, y) = tile_to_screen(cfg, cam, p.tile.0, p.tile.1);
            let d = (x - cx).powi(2) + (y - cy).powi(2);
            (
                d,
                [x, y, p.radius * per_tile, 0.0],
                [p.color.r, p.color.g, p.color.b, p.color.a],
            )
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (i, (_, at, color)) in near.into_iter().take(gpu::MAX_LIGHTS).enumerate() {
        u.lights[i] = at;
        u.colors[i] = color;
        u.count = (i + 1) as f32;
    }
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lights_nearest_the_center_win() {
        let cfg = RenderConfig::default();
        let light = |x: f32| PointLight {
            tile: (x, 20.0),
            radius: 3.0,
            color: Rgba::WHITE,
        };
        let l = Lighting {
            ambient: Rgba::new(0.2, 0.2, 0.4, 1.0),
            lights: (0..40).map(|i| light(10.0 + i as f32)).rev().collect(),
        };
        let u = uniforms(&cfg, &cam(), &l, None);
        assert_eq!(u.count as usize, gpu::MAX_LIGHTS);
        let (x, y) = tile_to_screen(&cfg, &cam(), 10.0, 20.0);
        assert_eq!(
            [u.lights[0][0], u.lights[0][1]],
            [x, y],
            "the one under the camera first"
        );
        assert_eq!(u.ambient, [0.2, 0.2, 0.4, 1.0]);
        assert!(u.lights[0][2] > 0.0);
    }

    fn cam() -> Camera {
        Camera {
            center: (10.0, 20.0),
            zoom: 1.5,
            screen: (800.0, 600.0),
        }
    }

    /// The shader's `sight_at` mapping, in Rust: a screen pixel back to
    /// its tile, then to the mask texel (`uv * side - 0.5` is the tile
    /// offset from the mask origin).
    #[test]
    fn the_sight_mask_lines_up_with_the_tiles() {
        let cfg = RenderConfig::default();
        let sight = SightMask {
            origin: TilePos::new(5, 12),
            side: 21,
            seen: vec![255; 21 * 21],
            shade: Rgba::WHITE,
        };
        let u = uniforms(&cfg, &cam(), &Lighting::default(), Some(&sight));
        assert_eq!(u.fog_box, [5.0, 12.0, 21.0, 1.0]);
        for (tx, ty) in [(10.0, 20.0), (7.0, 15.0), (13.5, 22.0)] {
            let (x, y) = tile_to_screen(&cfg, &cam(), tx, ty);
            let a = (x + u.fog_map[0]) / u.fog_map[2];
            let b = (y + u.fog_map[1]) / u.fog_map[3];
            let tile = ((a + b) / 2.0, (b - a) / 2.0);
            assert!(
                (tile.0 - tx).abs() < 1e-3 && (tile.1 - ty).abs() < 1e-3,
                "{tile:?}"
            );
            let texel = (tile.0 - u.fog_box[0], tile.1 - u.fog_box[1]);
            assert!((texel.0 - (tx - 5.0)).abs() < 1e-3 && (texel.1 - (ty - 12.0)).abs() < 1e-3);
        }
        let none = uniforms(&cfg, &cam(), &Lighting::default(), None);
        assert_eq!(none.fog_box[3], 0.0, "no mask: everything seen");
    }

    #[test]
    fn center_is_middle_of_screen() {
        let cfg = RenderConfig::default();
        let (x, y) = tile_to_screen(&cfg, &cam(), 10.0, 20.0);
        assert!((x - 400.0).abs() < 1e-3 && (y - 300.0).abs() < 1e-3);
    }

    #[test]
    fn inverse_round_trips() {
        let cfg = RenderConfig::default();
        for tx in -5..30 {
            for ty in 0..40 {
                let (x, y) = tile_to_screen(&cfg, &cam(), tx as f32, ty as f32);
                assert_eq!(screen_to_tile(&cfg, &cam(), x, y), TilePos::new(tx, ty));
                // Anywhere inside the diamond maps to the same tile.
                let q = cfg.tile_h / 2.0 * cam().zoom * 0.45;
                assert_eq!(
                    screen_to_tile(&cfg, &cam(), x + q, y + q / 2.0),
                    TilePos::new(tx, ty)
                );
            }
        }
    }

    #[test]
    fn one_tile_east_goes_right_and_down() {
        let cfg = RenderConfig::default();
        let (x0, y0) = tile_to_screen(&cfg, &cam(), 0.0, 0.0);
        let (x1, y1) = tile_to_screen(&cfg, &cam(), 1.0, 0.0);
        assert_eq!((x1 - x0, y1 - y0), (48.0, 24.0));
    }

    #[test]
    fn depth_order() {
        let item = |x: f32, y: f32, layer| ItemDesign {
            shape: Shape::Marker(Rgba::WHITE),
            tile: (x, y),
            layer,
        };
        let mut items = vec![
            item(5.0, 5.0, Layer::Object),
            item(9.0, 9.0, Layer::Floor),
            item(2.0, 3.0, Layer::Object),
            item(3.0, 2.0, Layer::Object),
            item(0.0, 0.0, Layer::Effect),
        ];
        sort_items(&mut items);
        let tiles: Vec<_> = items.iter().map(|i| i.tile).collect();
        assert_eq!(
            tiles,
            [(9.0, 9.0), (2.0, 3.0), (3.0, 2.0), (5.0, 5.0), (0.0, 0.0)]
        );
    }

    #[test]
    fn visible_range_covers_screen() {
        let cfg = RenderConfig::default();
        let (lo, hi) = visible_tiles(&cfg, &cam(), 1);
        assert!(lo.x <= 10 && hi.x >= 10 && lo.y <= 20 && hi.y >= 20);
        assert!(hi.x - lo.x < 60);
    }

    #[test]
    fn shapes_become_triangles_and_offscreen_is_culled() {
        let cfg = RenderConfig::default();
        let mut b = Batch::default();
        let on = |shape| ItemDesign {
            shape,
            tile: (10.0, 20.0),
            layer: Layer::Object,
        };
        b.item(&cfg, &cam(), &on(Shape::Floor(Rgba::WHITE)));
        assert_eq!(b.verts.len(), 6);
        b.item(&cfg, &cam(), &on(Shape::Wall(Rgba::WHITE)));
        assert_eq!(b.verts.len(), 6 + 18);
        let far = ItemDesign {
            tile: (500.0, 20.0),
            ..on(Shape::Floor(Rgba::WHITE))
        };
        b.item(&cfg, &cam(), &far);
        assert_eq!(b.verts.len(), 24);
    }
}
