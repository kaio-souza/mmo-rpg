//! What the HUD draws over the world (phase 5): names, health bars and
//! quest markers over heads, rising damage numbers; and the tooltip box
//! the windows share.

use crate::fonts::Role;
use crate::paint::Rect;
use crate::theme::Color;
use crate::ui::Frame;

/// What the player has to do with an NPC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    /// A quest to start: `!`.
    Available,
    /// A `Talk` objective: `!` in the secondary color.
    Talk,
    /// A quest to hand in: `?`.
    Ready,
    /// Phase 7: the NPC keeps a shop (right click opens it): `$`.
    Shop,
}

/// One being on screen: `at` is the screen point just over its head.
#[derive(Clone, Debug, PartialEq)]
pub struct Overhead {
    pub at: (f32, f32),
    pub name: Option<String>,
    /// Hundredths of the maximum; `None` hides the bar.
    pub health: Option<u8>,
    pub marker: Option<MarkerKind>,
    /// The selected target: name in the highlight color.
    pub target: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatKind {
    /// A blow of the player.
    Dealt,
    /// A blow on the player.
    Taken,
    Miss,
    /// Phase 9: a gesture (an emote), over anyone.
    Emote,
    /// An item that came into the backpack (gathered, crafted, bought,
    /// looted): its icon and `+N` over the player.
    Gained,
}

/// A number rising from a being; `age` goes from 0 (new) to 1 (gone).
#[derive(Clone, Debug, PartialEq)]
pub struct FloatText {
    pub at: (f32, f32),
    pub text: String,
    pub kind: FloatKind,
    pub crit: bool,
    pub age: f32,
    /// Protocol 20: an emote with a picture shows it in a bubble instead of
    /// the text.
    pub icon: Option<[f32; 4]>,
}

pub(crate) fn overheads(f: &mut Frame<'_>, list: &[Overhead]) {
    let s = f.theme.sizes.clone();
    let c = f.theme.colors.clone();
    let lh = f.line_height(Role::Text, s.small);
    let bar_w = f.px(40.0);
    let bar_h = f.px(4.0);
    for o in list {
        let (x, mut y) = o.at;
        if let Some(h) = o.health {
            y -= bar_h + f.px(2.0);
            let r = Rect::new(x - bar_w / 2.0, y, bar_w, bar_h);
            f.painter.bar(f.theme, r, u64::from(h.min(100)), 100, c.life);
        }
        if let Some(name) = &o.name {
            y -= lh;
            let color = if o.target { c.highlight } else { c.text };
            let px = f.px(s.small);
            let w = f.fonts.measure(Role::Text, px, name);
            f.painter.rect(
                Rect::new(x - w / 2.0 - f.px(3.0), y, w + f.px(6.0), lh),
                shade(c.background, 0.55),
            );
            f.label(Role::Text, s.small, (x - w / 2.0, y), color, name);
        }
        if let Some(m) = o.marker {
            let (glyph, color) = match m {
                MarkerKind::Available => ("!", c.highlight),
                MarkerKind::Talk => ("!", c.secondary),
                MarkerKind::Ready => ("?", c.highlight),
                MarkerKind::Shop => ("$", c.secondary),
            };
            let th = f.line_height(Role::Title, s.title);
            y -= th;
            let px = f.px(s.title);
            let w = f.fonts.measure(Role::Title, px, glyph);
            f.label(Role::Title, s.title, (x - w / 2.0, y), color, glyph);
        }
    }
}

pub(crate) fn floats(f: &mut Frame<'_>, list: &[FloatText]) {
    let s = f.theme.sizes.clone();
    let c = f.theme.colors.clone();
    let rise = f.px(36.0);
    for t in list {
        if let (FloatKind::Gained, Some(uv)) = (t.kind, t.icon) {
            gained(f, t, uv);
            continue;
        }
        if let Some(uv) = t.icon {
            emote_bubble(f, t, uv);
            continue;
        }
        let mut color = match t.kind {
            FloatKind::Dealt => c.text,
            FloatKind::Taken => c.life,
            FloatKind::Miss => c.dim_text,
            FloatKind::Emote | FloatKind::Gained => c.highlight,
        };
        if t.crit && t.kind == FloatKind::Dealt {
            color = c.highlight;
        }
        color[3] *= (1.0 - t.age).clamp(0.0, 1.0);
        let size = if t.crit { s.title } else { s.text };
        let role = if t.crit { Role::Title } else { Role::Numbers };
        let px = f.px(size);
        let w = f.fonts.measure(role, px, &t.text);
        let y = t.at.1 - rise * t.age;
        f.label(role, size, (t.at.0 - w / 2.0, y), color, &t.text);
    }
}

/// An item gained: its icon and `+N` rising from the head, fading out.
fn gained(f: &mut Frame<'_>, t: &FloatText, uv: [f32; 4]) {
    let s = f.theme.sizes.clone();
    let c = f.theme.colors.clone();
    let age = t.age.clamp(0.0, 1.0);
    let alpha = ((1.0 - age) / 0.35).min(1.0);
    let side = f.px(22.0);
    let px = f.px(s.text);
    let w = f.fonts.measure(Role::Numbers, px, &t.text);
    let gap = f.px(4.0);
    let x = t.at.0 - (side + gap + w) / 2.0;
    let y = t.at.1 - f.px(48.0) * age - side;
    f.painter
        .image(Rect::new(x, y, side, side), uv, [1.0, 1.0, 1.0, alpha]);
    let mut color = c.highlight;
    color[3] *= alpha;
    let ty = y + (side - f.line_height(Role::Numbers, s.text)) / 2.0;
    f.label(Role::Numbers, s.text, (x + side + gap, ty), color, &t.text);
}

/// An emote with a picture: the icon in a speech bubble over the head,
/// popping in, floating a little and fading at the end.
fn emote_bubble(f: &mut Frame<'_>, t: &FloatText, uv: [f32; 4]) {
    let c = f.theme.colors.clone();
    let age = t.age.clamp(0.0, 1.0);
    // Pops in during the first 10%, fades during the last 25%.
    let grow = (age / 0.1).min(1.0);
    let alpha = ((1.0 - age) / 0.25).min(1.0);
    let side = f.px(30.0) * (0.6 + 0.4 * grow);
    let pad = f.px(4.0);
    let x = t.at.0 - side / 2.0;
    let y = t.at.1 - side - pad * 2.0 - f.px(10.0) * age;
    let mut back = c.background;
    back[3] *= 0.85 * alpha;
    let mut edge = c.highlight;
    edge[3] *= alpha;
    let outer = Rect::new(x - pad, y - pad, side + pad * 2.0, side + pad * 2.0);
    f.painter.rect(
        Rect::new(outer.x - 1.0, outer.y - 1.0, outer.w + 2.0, outer.h + 2.0),
        edge,
    );
    f.painter.rect(outer, back);
    // The tail of the bubble, pointing at the head.
    let tail = (t.at.0, outer.y + outer.h + f.px(6.0));
    f.painter.tri(
        (t.at.0 - f.px(5.0), outer.y + outer.h),
        (t.at.0 + f.px(5.0), outer.y + outer.h),
        tail,
        back,
    );
    f.painter
        .image(Rect::new(x, y, side, side), uv, [1.0, 1.0, 1.0, alpha]);
}

/// A box of lines next to the mouse, kept inside the screen. The first
/// line is the title. Drawn at the end of the frame, over every window.
pub(crate) fn tooltip(f: &mut Frame<'_>, lines: &[(String, Color)]) {
    f.tip = Some(lines.to_vec());
}

pub(crate) fn draw_tooltip(f: &mut Frame<'_>, lines: &[(String, Color)]) {
    if lines.is_empty() {
        return;
    }
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let px = f.px(s.small);
    let max_w = f.px(300.0);
    let mut rows: Vec<(String, Color, Role)> = Vec::new();
    for (i, (text, color)) in lines.iter().enumerate() {
        let role = if i == 0 { Role::Title } else { Role::Text };
        for l in f.fonts.wrap(role, px, text, max_w) {
            rows.push((l, *color, role));
        }
    }
    let w = rows
        .iter()
        .map(|(t, _, role)| f.fonts.measure(*role, px, t))
        .fold(0.0, f32::max)
        + 2.0 * pad;
    let lh = f.line_height(Role::Text, s.small);
    let h = rows.len() as f32 * lh + 2.0 * pad;
    let (mx, my) = f.input.mouse;
    let x = (mx + pad).min(f.input.screen.0 - w).max(0.0);
    let y = if my + pad + h > f.input.screen.1 {
        (my - h - pad).max(0.0)
    } else {
        my + pad
    };
    let r = Rect::new(x, y, w, h);
    f.panel(r, true);
    for (i, (text, color, role)) in rows.iter().enumerate() {
        f.label(
            *role,
            s.small,
            (r.x + pad, r.y + pad + lh * i as f32),
            *color,
            text,
        );
    }
}

fn shade(c: Color, alpha: f32) -> Color {
    [c[0], c[1], c[2], alpha]
}
