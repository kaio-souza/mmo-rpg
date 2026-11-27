//! `WorldState` → draw list. Phase 6b: floors, walls and beings come from
//! the sprite sheet by name (`graphics`, `assets/tiles.ron`); anything
//! without an image keeps the flat test look, so a missing file never hides
//! the game.
//!
//! Phase 10: the lights of the frame too: shining floors and sprites (drawn
//! glowing) and the light the player carries.
//!
//! Ground the player cannot see (beyond the view radius or behind
//! what blocks sight, the server's own rule) is drawn shaded, so a monster
//! that vanishes there plainly went out of sight. The ground takes it from
//! a sight mask the renderer blends between tile centers (a soft edge, not
//! tile steps); walls and glowing tiles take the same smoothed value as a
//! tint. When the sight changes (a step) the old mask fades into the new
//! one along the step being drawn, so the edge glides with the player
//! instead of jumping a tile.

use crate::graphics::{Graphics, Pose};
use base::TilePos;
use client_render::{
    Camera, ItemDesign, Layer, PointLight, RenderConfig, Rgba, Shape, SightMask, visible_tiles,
};
use client_state::WorldState;
use protocol::{EntityKind, TILE_BLOCKS, TILE_FLOOR};

const FLOOR: Rgba = Rgba::new(0.22, 0.30, 0.21, 1.0);
const WALL: Rgba = Rgba::new(0.45, 0.42, 0.40, 1.0);
const ME: Rgba = Rgba::new(0.35, 0.85, 0.45, 1.0);
const PLAYER: Rgba = Rgba::new(0.35, 0.60, 0.95, 1.0);
const NPC: Rgba = Rgba::new(0.90, 0.45, 0.35, 1.0);
const MONSTER: Rgba = Rgba::new(0.75, 0.20, 0.65, 1.0);
const DESTINATION: Rgba = Rgba::new(1.0, 0.9, 0.3, 0.8);
const TARGET: Rgba = Rgba::new(0.95, 0.25, 0.2, 0.9);
/// `0xRRGGBB` as a tint.
fn rgba(c: u32) -> Rgba {
    let ch = |s: u32| ((c >> s) & 0xff) as f32 / 255.0;
    Rgba::new(ch(16), ch(8), ch(0), 1.0)
}

/// What unseen ground is multiplied by: darker and a little cold.
const FOG: Rgba = Rgba::new(0.42, 0.44, 0.55, 1.0);

/// Blank tiles around the square of sight, so the blur fades out inside
/// the mask and anything past it reads as unseen.
const FOG_BORDER: i32 = 2;

/// Which tiles around the player are in sight, worked out again only when
/// the player's tile, the terrain or the radius changes.
#[derive(Default)]
pub struct Fog {
    key: Option<(TilePos, usize, u16)>,
    center: TilePos,
    radius: i32,
    seen: Vec<bool>,
    /// The same, blurred (0..=255), with `FOG_BORDER` blank tiles around.
    mask: Option<SightMask>,
    /// What was shown when the sight last changed: it fades into `mask`.
    before: Option<SightMask>,
    /// What is drawn this frame: `before` and `mask` blended.
    shown: Option<SightMask>,
}

impl Fog {
    fn update(&mut self, state: &WorldState, now_ms: f64) {
        let Some((me, progress)) = state.me().map(|e| (e.pos, e.step_progress(now_ms))) else {
            self.key = None;
            return;
        };
        let key = (me, state.chunk_count(), state.view_radius());
        if self.key != Some(key) {
            self.key = Some(key);
            self.before = self.shown.take();
            self.recompute(state, me);
        }
        self.blend(progress);
    }

    fn recompute(&mut self, state: &WorldState, me: TilePos) {
        self.center = me;
        self.radius = i32::from(state.view_radius());
        let side = 2 * self.radius + 1;
        self.seen.clear();
        for dy in -self.radius..=self.radius {
            for dx in -self.radius..=self.radius {
                self.seen.push(state.sees(me, TilePos::new(me.x + dx, me.y + dy)));
            }
        }
        debug_assert_eq!(self.seen.len(), (side * side) as usize);
        self.mask = Some(self.blurred());
    }

    /// `before` toward `mask` by `t` (0..=1), each read on the new square;
    /// outside the old square counts as unseen.
    fn blend(&mut self, t: f64) {
        let Some(to) = &self.mask else {
            self.shown = None;
            return;
        };
        let from = self.before.as_ref().filter(|_| t < 1.0);
        let Some(from) = from else {
            self.before = None;
            if self.shown.as_ref() != Some(to) {
                self.shown = Some(to.clone());
            }
            return;
        };
        let side = to.side as i32;
        let (fside, k) = (from.side as i32, t as f32);
        let mut seen = Vec::with_capacity(to.seen.len());
        for dy in 0..side {
            for dx in 0..side {
                let (x, y) = (to.origin.x + dx - from.origin.x, to.origin.y + dy - from.origin.y);
                let old = if (0..fside).contains(&x) && (0..fside).contains(&y) {
                    f32::from(from.seen[(y * fside + x) as usize])
                } else {
                    0.0
                };
                let new = f32::from(to.seen[(dy * side + dx) as usize]);
                seen.push((old + (new - old) * k).round() as u8);
            }
        }
        self.shown = Some(SightMask {
            origin: to.origin,
            side: to.side,
            seen,
            shade: to.shade,
        });
    }

    /// The seen tiles as bytes, softened by a 3×3 [1 2 1] blur so the edge
    /// fades over about two tiles.
    fn blurred(&self) -> SightMask {
        let r = self.radius + FOG_BORDER;
        let side = 2 * r + 1;
        let raw = |dx: i32, dy: i32| -> u32 {
            u32::from(self.sees(TilePos::new(self.center.x + dx, self.center.y + dy))) * 255
        };
        let mut seen = Vec::with_capacity((side * side) as usize);
        for dy in -r..=r {
            for dx in -r..=r {
                let mut sum = 0;
                for (oy, wy) in [(-1, 1), (0, 2), (1, 1)] {
                    for (ox, wx) in [(-1, 1), (0, 2), (1, 1)] {
                        sum += raw(dx + ox, dy + oy) * wx * wy;
                    }
                }
                seen.push(u8::try_from(sum / 16).unwrap_or(u8::MAX));
            }
        }
        SightMask {
            origin: TilePos::new(self.center.x - r, self.center.y - r),
            side: side as u32,
            seen,
            shade: FOG,
        }
    }

    /// For the renderer: `None` when sight is not known (all seen).
    pub fn mask(&self) -> Option<&SightMask> {
        self.shown
            .as_ref()
            .filter(|_| self.key.is_some() && self.radius > 0)
    }

    /// Tint of a standing tile (wall, glow): the smoothed sight at it.
    fn tint(&self, p: TilePos) -> Rgba {
        let Some(m) = self.mask() else {
            return Rgba::WHITE;
        };
        let (dx, dy) = (p.x - m.origin.x, p.y - m.origin.y);
        let side = m.side as i32;
        let k = if (0..side).contains(&dx) && (0..side).contains(&dy) {
            f32::from(m.seen[(dy * side + dx) as usize]) / 255.0
        } else {
            0.0
        };
        let mix = |fog: f32| fog + (1.0 - fog) * k;
        Rgba::new(mix(FOG.r), mix(FOG.g), mix(FOG.b), 1.0)
    }

    /// Not known (no player yet or no radius): everything is seen.
    fn sees(&self, p: TilePos) -> bool {
        if self.key.is_none() || self.radius == 0 {
            return true;
        }
        let (dx, dy) = (p.x - self.center.x, p.y - self.center.y);
        if dx.abs() > self.radius || dy.abs() > self.radius {
            return false;
        }
        let side = 2 * self.radius + 1;
        let i = (dy + self.radius) * side + dx + self.radius;
        usize::try_from(i)
            .ok()
            .and_then(|i| self.seen.get(i))
            .copied()
            .unwrap_or(false)
    }
}

/// Up to four seconds of offset per tile, the same every frame.
fn scatter(x: i32, y: i32) -> f64 {
    let h = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b);
    f64::from(h >> 24) / 64.0
}

fn shade(c: Rgba, k: Rgba) -> Rgba {
    Rgba::new(c.r * k.r, c.g * k.g, c.b * k.b, c.a)
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    state: &WorldState,
    cfg: &RenderConfig,
    cam: &Camera,
    now_ms: f64,
    destination: Option<TilePos>,
    target: Option<base::EntityId>,
    (gfx, night): (&Graphics, bool),
    (items, lights, fog): (&mut Vec<ItemDesign>, &mut Vec<PointLight>, &mut Fog),
) {
    items.clear();
    lights.clear();
    fog.update(state, now_ms);
    let (lo, hi) = visible_tiles(cfg, cam, 2);
    for y in lo.y..=hi.y {
        for x in lo.x..=hi.x {
            let Some(word) = state.tile(TilePos::new(x, y)) else {
                continue;
            };
            let tile = (x as f32, y as f32);
            let blocks = word & TILE_BLOCKS != 0;
            let look = gfx.tile(word & TILE_FLOOR);
            let wall = look.map_or(blocks, |l| l.wall);
            // A fire put out by day shows its own sprite, without light.
            let by_day = look.and_then(|l| l.day_sprite.as_deref()).filter(|_| !night);
            let shine = look.and_then(|l| l.light).filter(|_| by_day.is_none());
            // The ground is shaded by the renderer from the sight mask;
            // walls and glowing tiles skip it and are tinted here.
            let tint = if wall || shine.is_some() {
                fog.tint(TilePos::new(x, y))
            } else {
                Rgba::WHITE
            };
            // Animated scenery (fire, water): each tile a little out of step.
            let pose = Pose::still(now_ms / 1000.0 + scatter(x, y));
            let sprite = look.and_then(|l| gfx.sprite_at(by_day.unwrap_or(&l.sprite), tint, pose));
            if let Some(l) = shine {
                lights.push(PointLight {
                    tile,
                    radius: l.radius,
                    color: l.rgba(),
                });
            }
            let shape = match sprite {
                Some(s) if shine.is_some() => Shape::Glow(s),
                Some(s) => Shape::Sprite(s),
                None if wall => Shape::Wall(shade(WALL, tint)),
                None => Shape::Floor(shade(FLOOR, tint)),
            };
            // A standing tile still needs ground under it.
            if wall && sprite.is_some() {
                items.push(ItemDesign {
                    shape: Shape::Floor(FLOOR),
                    tile,
                    layer: Layer::Floor,
                });
            }
            let layer = if wall { Layer::Object } else { Layer::Floor };
            items.push(ItemDesign { shape, tile, layer });
        }
    }
    let me = state.my_id();
    for e in state.entities() {
        let (x, y) = e.visual_pos(now_ms);
        let tile = (x as f32, y as f32);
        let skin = crate::interface::skin_of(state, e.look).map(rgba);
        let color = match e.kind {
            EntityKind::Player if skin.is_some() => skin.unwrap_or(PLAYER),
            _ if Some(e.id) == me => ME,
            EntityKind::Player => PLAYER,
            EntityKind::Npc | EntityKind::Node => NPC,
            EntityKind::Monster => MONSTER,
        };
        // Monsters darken as they lose HP.
        let shade = match e.kind {
            EntityKind::Monster => 0.35 + 0.65 * f32::from(e.health.min(100)) / 100.0,
            _ => 1.0,
        };
        // A player is layers (body, clothes, hair); the first is the shape
        // below, the others go over it.
        let layers = match e.kind {
            EntityKind::Player => crate::interface::look_layers(state, e.look),
            _ => Vec::new(),
        };
        let (name, tint) = match e.kind {
            EntityKind::Player => (
                layers.first().map(|l| l.0),
                layers.first().and_then(|l| l.1).map_or(Rgba::WHITE, rgba),
            ),
            _ => (state.sprite_name(e.appearance), Rgba::WHITE),
        };
        let tint = Rgba::new(tint.r * shade, tint.g * shade, tint.b * shade, tint.a);
        let shine = name.and_then(|n| gfx.sprite_light(n));
        if let Some(l) = shine {
            lights.push(PointLight {
                tile,
                radius: l.radius,
                color: l.rgba(),
            });
        }
        // Phase 11: the light of an equipped item (a torch).
        if let Some(l) = e
            .light
            .checked_sub(1)
            .and_then(|i| state.item(base::ItemId::new(i)))
            .and_then(|i| i.light)
        {
            let [r, g, b] = l.color.map(|c| f32::from(c) / 255.0);
            lights.push(PointLight {
                tile,
                radius: f32::from(l.radius),
                color: Rgba::new(r, g, b, 1.0),
            });
        }
        if Some(e.id) == me
            && let Some(l) = gfx.carried_light()
        {
            lights.push(PointLight {
                tile,
                radius: l.radius,
                color: l.rgba(),
            });
        }
        let pose = Pose {
            secs: now_ms / 1000.0 + f64::from(e.id.get() % 97) / 97.0,
            facing: e.direction,
            moving: e.moving(now_ms),
        };
        let shape = match name.and_then(|n| gfx.sprite_at(n, tint, pose)) {
            Some(s) if shine.is_some() => Shape::Glow(s),
            Some(s) => Shape::Sprite(s),
            None => Shape::Figure(Rgba::new(color.r * shade, color.g * shade, color.b * shade, 1.0)),
        };
        items.push(ItemDesign {
            shape,
            tile,
            layer: Layer::Object,
        });
        for (layer, tone) in layers.iter().skip(1) {
            let t = tone.map_or(Rgba::WHITE, rgba);
            let t = Rgba::new(t.r * shade, t.g * shade, t.b * shade, t.a);
            if let Some(s) = gfx.sprite_at(layer, t, pose) {
                items.push(ItemDesign {
                    shape: Shape::Sprite(s),
                    tile,
                    layer: Layer::Object,
                });
            }
        }
        // Armor, then off hand and weapon, over the body (same depth:
        // drawn in order).
        let (weapon, armor, offhand) = e.equipment;
        for v in [armor, offhand, weapon] {
            if let Some(s) = state
                .sprite_name(v)
                .and_then(|n| gfx.sprite_at(n, Rgba::WHITE, pose))
            {
                items.push(ItemDesign {
                    shape: Shape::Sprite(s),
                    tile,
                    layer: Layer::Object,
                });
            }
        }
    }
    // The selected target: a red outline under its feet.
    if let Some(e) = target.and_then(|t| state.entity(t)) {
        let (x, y) = e.visual_pos(now_ms);
        items.push(ItemDesign {
            shape: Shape::Marker(TARGET),
            tile: (x as f32, y as f32),
            layer: Layer::Effect,
        });
    }
    if let Some(d) = destination {
        items.push(ItemDesign {
            shape: Shape::Marker(DESTINATION),
            tile: (d.x as f32, d.y as f32),
            layer: Layer::Effect,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Radius 3 around (10, 10); everything in the square seen.
    fn fog() -> Fog {
        let radius = 3;
        let side = (2 * radius + 1) as usize;
        let mut f = Fog {
            key: Some((TilePos::new(10, 10), 1, radius as u16)),
            center: TilePos::new(10, 10),
            radius,
            seen: vec![true; side * side],
            mask: None,
            before: None,
            shown: None,
        };
        f.mask = Some(f.blurred());
        f.blend(1.0);
        f
    }

    #[test]
    fn the_sight_mask_fades_at_the_edge() {
        let f = fog();
        let m = f.mask().expect("mask");
        let side = m.side as i32;
        assert_eq!(side, 2 * (3 + FOG_BORDER) + 1);
        let at = |x: i32, y: i32| m.seen[((y - m.origin.y) * side + x - m.origin.x) as usize];
        assert_eq!(at(10, 10), 255, "the middle is in plain sight");
        assert_eq!(at(10 + 3 + FOG_BORDER, 10), 0, "past the square is not");
        // The last seen tile and the first unseen one share the edge.
        let (inside, outside) = (at(13, 10), at(14, 10));
        assert!(
            0 < outside && outside < inside && inside < 255,
            "{inside} {outside}"
        );
    }

    /// A step east: the old edge fades into the new one along the step.
    #[test]
    fn the_sight_edge_glides_with_the_step() {
        let mut f = fog();
        let at = |m: &SightMask, x: i32, y: i32| {
            let side = m.side as i32;
            m.seen[((y - m.origin.y) * side + x - m.origin.x) as usize]
        };
        let old = at(f.mask().unwrap(), 14, 10);
        f.before = f.shown.take();
        f.center = TilePos::new(11, 10);
        f.key = Some((f.center, 1, 3));
        f.mask = Some(f.blurred());
        let new = at(f.mask.as_ref().unwrap(), 14, 10);
        assert!(old < new, "{old} {new}");
        f.blend(0.0);
        assert_eq!(
            at(f.mask().unwrap(), 14, 10),
            old,
            "the step starts on the old sight"
        );
        f.blend(0.5);
        let mid = at(f.mask().unwrap(), 14, 10);
        assert!(old < mid && mid < new, "{old} {mid} {new}");
        f.blend(1.0);
        assert_eq!(f.mask(), f.mask.as_ref(), "and ends on the new one");
    }

    #[test]
    fn walls_take_the_smoothed_tint() {
        let f = fog();
        assert_eq!(f.tint(TilePos::new(10, 10)), Rgba::WHITE);
        assert_eq!(f.tint(TilePos::new(40, 40)), FOG);
        let edge = f.tint(TilePos::new(14, 10));
        assert!(FOG.r < edge.r && edge.r < 1.0);
    }
}
