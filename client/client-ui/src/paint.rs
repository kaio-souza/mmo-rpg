//! Triangles in screen pixels: flat shapes (`uv` negative) and glyphs
//! (`uv` in the atlas). The binary copies them into `client-render`.

use crate::fonts::{ATLAS_SIZE, Fonts, Role};
use crate::theme::{Color, UiTheme};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.0 < self.x + self.w && p.1 >= self.y && p.1 < self.y + self.h
    }

    pub fn shrink(&self, by: f32) -> Rect {
        Rect::new(
            self.x + by,
            self.y + by,
            (self.w - 2.0 * by).max(0.0),
            (self.h - 2.0 * by).max(0.0),
        )
    }
}

const FLAT: [f32; 2] = [-1.0, -1.0];

fn alpha(c: Color, k: f32) -> Color {
    [c[0], c[1], c[2], c[3] * k]
}

#[derive(Default)]
pub struct Painter {
    pub verts: Vec<Vertex>,
}

impl Painter {
    pub fn clear(&mut self) {
        self.verts.clear();
    }

    /// Fades what was drawn since vertex `from` to `k` (0..=1) of its
    /// alpha and moves it down by `dy`: a window opening or closing.
    pub fn fade_since(&mut self, from: usize, k: f32, dy: f32) {
        if k >= 1.0 && dy == 0.0 {
            return;
        }
        for v in self.verts.iter_mut().skip(from) {
            v.color[3] *= k;
            v.pos[1] += dy;
        }
    }

    pub fn tri(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32), color: Color) {
        for p in [a, b, c] {
            self.verts.push(Vertex {
                pos: [p.0, p.1],
                uv: FLAT,
                color,
            });
        }
    }

    /// An image of the sprite sheet (`uv` from the binary, `u` shifted by 2:
    /// icons, phase 6b) tinted by `color` (white: unchanged).
    pub fn image(&mut self, r: Rect, uv: [f32; 4], color: Color) {
        let [u0, v0, u1, v1] = uv;
        let v = |x: f32, y: f32, u: f32, w: f32| Vertex {
            pos: [x, y],
            uv: [u, w],
            color,
        };
        let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
        self.verts.extend([
            v(x0, y0, u0, v0),
            v(x1, y0, u1, v0),
            v(x1, y1, u1, v1),
            v(x0, y0, u0, v0),
            v(x1, y1, u1, v1),
            v(x0, y1, u0, v1),
        ]);
    }

    pub fn rect(&mut self, r: Rect, color: Color) {
        let (a, b, c, d) = (
            (r.x, r.y),
            (r.x + r.w, r.y),
            (r.x + r.w, r.y + r.h),
            (r.x, r.y + r.h),
        );
        self.tri(a, b, c, color);
        self.tri(a, c, d, color);
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, color: Color) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        let q = [
            (a.0 + nx, a.1 + ny),
            (b.0 + nx, b.1 + ny),
            (b.0 - nx, b.1 - ny),
            (a.0 - nx, a.1 - ny),
        ];
        self.tri(q[0], q[1], q[2], color);
        self.tri(q[0], q[2], q[3], color);
    }

    fn outline(&mut self, pts: &[(f32, f32)], width: f32, color: Color) {
        for i in 0..pts.len() {
            self.line(pts[i], pts[(i + 1) % pts.len()], width, color);
        }
    }

    fn fill(&mut self, pts: &[(f32, f32)], color: Color) {
        for i in 1..pts.len().saturating_sub(1) {
            self.tri(pts[0], pts[i], pts[i + 1], color);
        }
    }

    /// The theme panel: translucent, two corners cut on the diagonal, a
    /// border and its glow. `strong` uses the strong border color.
    pub fn panel(&mut self, theme: &UiTheme, r: Rect, strong: bool) {
        let s = theme.panel.diagonal_slash.min(r.w / 3.0).min(r.h / 3.0).max(0.0);
        let pts = [
            (r.x + s, r.y),
            (r.x + r.w, r.y),
            (r.x + r.w, r.y + r.h - s),
            (r.x + r.w - s, r.y + r.h),
            (r.x, r.y + r.h),
            (r.x, r.y + s),
        ];
        self.fill(&pts, theme.colors.panel);
        let border = if strong {
            theme.colors.strong_border
        } else {
            theme.colors.border
        };
        let glow = theme.panel.border_glow;
        if glow > 0.0 {
            for (spread, k) in [(5.0, 0.12), (3.0, 0.22), (1.5, 0.4)] {
                self.outline(
                    &pts,
                    theme.panel.border_thickness + spread,
                    alpha(theme.colors.strong_border, k * glow),
                );
            }
        }
        self.outline(&pts, theme.panel.border_thickness.max(0.5), border);
    }

    /// A bar filled `value / max`, with the theme border.
    pub fn bar(&mut self, theme: &UiTheme, r: Rect, value: u64, max: u64, color: Color) {
        self.rect(r, alpha(color, 0.18));
        let k = if max == 0 {
            0.0
        } else {
            (value.min(max) as f32) / (max as f32)
        };
        self.rect(Rect::new(r.x, r.y, r.w * k, r.h), color);
        self.outline(
            &[
                (r.x, r.y),
                (r.x + r.w, r.y),
                (r.x + r.w, r.y + r.h),
                (r.x, r.y + r.h),
            ],
            1.0,
            theme.colors.border,
        );
    }

    /// One line of text with its top-left corner at `(x, y)`; returns the
    /// width drawn.
    pub fn text(
        &mut self,
        fonts: &mut Fonts,
        role: Role,
        px: f32,
        (x, y): (f32, f32),
        color: Color,
        text: &str,
    ) -> f32 {
        let base = (y + fonts.ascent(role, px)).round();
        let mut pen = x.round();
        let k = 1.0 / ATLAS_SIZE as f32;
        for c in text.chars() {
            let Some(g) = fonts.glyph(role, c, px) else {
                continue;
            };
            if g.w > 0 && g.h > 0 {
                let x0 = pen + g.xmin;
                let y0 = base - g.ymin - g.h as f32;
                let (x1, y1) = (x0 + g.w as f32, y0 + g.h as f32);
                let (u0, v0) = (g.x as f32 * k, g.y as f32 * k);
                let (u1, v1) = ((g.x + g.w) as f32 * k, (g.y + g.h) as f32 * k);
                let v = |pos: (f32, f32), uv: (f32, f32)| Vertex {
                    pos: [pos.0, pos.1],
                    uv: [uv.0, uv.1],
                    color,
                };
                self.verts.extend([
                    v((x0, y0), (u0, v0)),
                    v((x1, y0), (u1, v0)),
                    v((x1, y1), (u1, v1)),
                    v((x0, y0), (u0, v0)),
                    v((x1, y1), (u1, v1)),
                    v((x0, y1), (u0, v1)),
                ]);
            }
            pen += g.advance;
        }
        pen - x
    }

    /// Text from the top-left of `area` that does not fit in its width
    /// ends in "…".
    pub fn text_fit(
        &mut self,
        fonts: &mut Fonts,
        (role, px): (Role, f32),
        area: Rect,
        color: Color,
        text: &str,
    ) {
        let (at, width) = ((area.x, area.y), area.w);
        if fonts.measure(role, px, text) <= width {
            self.text(fonts, role, px, at, color, text);
            return;
        }
        let ell = fonts.measure(role, px, "…");
        let mut cut = String::new();
        let mut w = 0.0;
        for c in text.chars() {
            let a = fonts.measure(role, px, c.encode_utf8(&mut [0; 4]));
            if w + a + ell > width {
                break;
            }
            w += a;
            cut.push(c);
        }
        cut.push('…');
        self.text(fonts, role, px, at, color, &cut);
    }
}
