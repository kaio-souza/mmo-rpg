//! The HUD windows: inventory, shop, status, quests, techniques, help,
//! dialogue and the shared window frame and scrolling list.

use super::*;

/// The techniques the character knows (K), right over the hotbar: click
/// one to put it in the first empty slot, or drag it to a slot.
pub(super) fn techniques(f: &mut Frame<'_>, m: &HudModel, drag: &mut Option<Drag>) -> bool {
    let s = f.theme.sizes.clone();
    let (pad, slot) = (f.px(s.padding), f.px(s.hotbar_slot));
    let lh = f.line_height(Role::Text, s.small);
    let cols = 10usize;
    let rows = m.techniques.len().div_ceil(cols).max(1);
    let w = cols as f32 * (slot + pad / 2.0) + 1.5 * pad;
    let h = f.line_height(Role::Title, s.title) + rows as f32 * (slot + pad / 2.0) + lh + 3.0 * pad;
    let bar_top = hotbar_rect(f, 0).y;
    let r = Rect::new((f.input.screen.0 - w) / 2.0, bar_top - h - pad, w, h);
    let (y, closed) = window(
        f,
        r,
        "ui.techniques.title",
        Some(&key_of(m, MenuWindow::Techniques)),
    );
    let c = f.theme.colors.clone();
    if m.techniques.is_empty() {
        let none = f.tr("ui.techniques.none");
        f.label_fit(
            Role::Text,
            s.small,
            (r.x + pad, y),
            r.w - 2.0 * pad,
            c.dim_text,
            &none,
        );
        return closed;
    }
    let mut tip = None;
    for (i, t) in m.techniques.iter().enumerate() {
        let sr = Rect::new(
            r.x + pad + (i % cols) as f32 * (slot + pad / 2.0),
            y + (i / cols) as f32 * (slot + pad / 2.0),
            slot,
            slot,
        );
        let hover = sr.contains(f.input.mouse);
        f.panel(sr, hover || !t.in_bar);
        match t.icon {
            Some(uv) => {
                let k = if t.in_bar { 0.6 } else { 1.0 };
                f.painter.image(sr.shrink(pad / 4.0), uv, [k, k, k, 1.0]);
            }
            None => f.label_fit(
                Role::Text,
                s.small,
                (sr.x + pad / 4.0, sr.y + pad / 4.0),
                slot - pad / 2.0,
                c.text,
                &t.name,
            ),
        }
        let level = t.level.to_string();
        f.label(
            Role::Numbers,
            s.small,
            (sr.x + pad / 4.0, sr.y + slot - lh),
            c.highlight,
            &level,
        );
        if hover && drag.is_none() {
            let mut lines = vec![(t.name.clone(), c.highlight)];
            lines.extend(t.tooltip.iter().map(|l| (l.clone(), c.text)));
            tip = Some(lines);
        }
        if drag.is_none() && f.drag_started(sr) {
            *drag = Some(Drag {
                from: DragSource::Technique(t.id),
                icon: t.icon,
                label: t.name.clone(),
            });
        } else if drag.is_none() && f.tapped(sr) && !t.in_bar {
            f.actions.push(UiAction::HotbarAdd(t.id));
        }
    }
    let hint = f.tr("ui.techniques.hint");
    f.label_fit(
        Role::Text,
        s.small,
        (r.x + pad, r.y + r.h - lh - pad),
        r.w - 2.0 * pad,
        c.dim_text,
        &hint,
    );
    if let Some(lines) = tip {
        overlay::tooltip(f, &lines);
    }
    closed
}

/// Id of the field that names a companion.
const COMPANION_NAME_FIELD: u32 = 300;

/// The bestiary (B): every monster the character defeated or tamed, with
/// how many times.
pub(super) fn bestiary(f: &mut Frame<'_>, m: &HudModel) -> bool {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.small);
    let icon = 2.0 * lh;
    let row = icon + pad / 2.0;
    let shown = m.bestiary.len().clamp(1, 12) as f32;
    let w = f.px(380.0).min(f.input.screen.0 - 2.0 * pad);
    let h = f.line_height(Role::Title, s.title) + shown * row + 2.0 * pad;
    let r = Rect::new((f.input.screen.0 - w) / 2.0, (f.input.screen.1 - h) / 3.0, w, h);
    let (y, closed) = window(f, r, "ui.bestiary.title", Some(&key_of(m, MenuWindow::Bestiary)));
    let c = f.theme.colors.clone();
    if m.bestiary.is_empty() {
        let none = f.tr("ui.bestiary.none");
        f.label_fit(
            Role::Text,
            s.small,
            (r.x + pad, y),
            r.w - 2.0 * pad,
            c.dim_text,
            &none,
        );
        return closed;
    }
    for (i, b) in m.bestiary.iter().take(12).enumerate() {
        let top = y + i as f32 * row;
        let ir = Rect::new(r.x + pad, top, icon, icon);
        f.panel(ir, false);
        if let Some(uv) = b.icon {
            f.painter.image(ir.shrink(pad / 4.0), uv, [1.0, 1.0, 1.0, 1.0]);
        }
        let tx = ir.x + icon + pad / 2.0;
        let tw = r.x + r.w - tx - pad;
        let name = f.tr_args(
            "ui.companions.name",
            &[base::Arg::Str(b.name.clone()), base::Arg::Int(i64::from(b.level))],
        );
        f.label_fit(Role::Text, s.small, (tx, top), tw, c.highlight, &name);
        let counts = f.tr_args(
            "ui.bestiary.counts",
            &[
                base::Arg::Int(i64::try_from(b.kills).unwrap_or(i64::MAX)),
                base::Arg::Int(i64::try_from(b.tames).unwrap_or(i64::MAX)),
            ],
        );
        f.label_fit(Role::Text, s.small, (tx, top + lh), tw, c.text, &counts);
    }
    closed
}

/// The companions window (P): every tamed monster with what it is doing
/// and its buttons. Call makes it follow (the one out goes back), Send back
/// dismisses it, Release lets it go after a second click.
pub(super) fn companions(
    f: &mut Frame<'_>,
    m: &HudModel,
    confirm: &mut Option<u8>,
    naming: &mut Option<(u8, String)>,
) -> bool {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.small);
    let bh = f.line_height(Role::Text, s.text) + pad / 2.0;
    let icon = 2.0 * lh + pad / 2.0;
    let row = icon.max(bh) + pad;
    let rows = m.companions.len().max(1) as f32;
    let w = f.px(420.0).min(f.input.screen.0 - 2.0 * pad);
    let h = f.line_height(Role::Title, s.title) + rows * row + lh + 3.0 * pad;
    let r = Rect::new((f.input.screen.0 - w) / 2.0, (f.input.screen.1 - h) / 3.0, w, h);
    let (y, closed) = window(
        f,
        r,
        "ui.companions.title",
        Some(&key_of(m, MenuWindow::Companions)),
    );
    let c = f.theme.colors.clone();
    if m.companions.is_empty() {
        let none = f.tr("ui.companions.none");
        for (i, line) in f
            .fonts
            .wrap(Role::Text, f.px(s.small), &none, r.w - 2.0 * pad)
            .iter()
            .enumerate()
        {
            f.label(
                Role::Text,
                s.small,
                (r.x + pad, y + i as f32 * lh),
                c.dim_text,
                line,
            );
        }
        return closed;
    }
    let (call, back, release, sure) = (
        f.tr("ui.companions.call"),
        f.tr("ui.companions.send_back"),
        f.tr("ui.companions.release"),
        f.tr("ui.companions.release_sure"),
    );
    let bw = f.px(96.0);
    for (i, comp) in m.companions.iter().enumerate() {
        let index = u8::try_from(i).unwrap_or(u8::MAX);
        let top = y + i as f32 * row;
        let ir = Rect::new(r.x + pad, top, icon, icon);
        f.panel(ir, comp.out);
        if let Some(uv) = comp.icon {
            f.painter.image(ir.shrink(pad / 4.0), uv, [1.0, 1.0, 1.0, 1.0]);
        }
        let tx = ir.x + icon + pad / 2.0;
        let text_w = r.x + r.w - tx - 2.0 * bw - 2.0 * pad;
        // The name, or a field to type a new one (Enter keeps it, empty
        // gives back the monster's).
        match naming.as_mut().filter(|(i, _)| *i == index) {
            Some((_, draft)) => {
                let fr = Rect::new(tx, top, text_w, lh + pad / 2.0);
                if f.field(COMPANION_NAME_FIELD, fr, draft, false, 16) {
                    f.actions
                        .push(UiAction::NameCompanion(index, draft.trim().to_owned()));
                    *naming = None;
                }
            }
            None => {
                let name = f.tr_args(
                    "ui.companions.name",
                    &[
                        base::Arg::Str(comp.name.clone()),
                        base::Arg::Int(i64::from(comp.level)),
                    ],
                );
                let nw = text_w - f.px(24.0);
                f.label_fit(Role::Text, s.small, (tx, top), nw, c.highlight, &name);
                // A small pen after the name starts naming it.
                let pen = Rect::new(tx + nw, top, f.px(22.0), lh);
                if f.button(pen, "✎") {
                    *naming = Some((index, comp.given.clone().unwrap_or_default()));
                    *f.focus = Some(COMPANION_NAME_FIELD);
                }
                if pen.contains(f.input.mouse) {
                    let rename = f.tr("ui.companions.rename");
                    overlay::tooltip(f, &[(rename, c.text)]);
                }
            }
        }
        let (state, color) = if comp.out {
            (f.tr("ui.companions.following"), c.text)
        } else if comp.rest_secs > 0 {
            (
                f.tr_args(
                    "ui.companions.resting",
                    &[base::Arg::Int(i64::from(comp.rest_secs))],
                ),
                c.error,
            )
        } else {
            (f.tr("ui.companions.ready"), c.dim_text)
        };
        f.label_fit(Role::Text, s.small, (tx, top + lh), text_w, color, &state);
        // Call or Send back, then Release.
        let b1 = Rect::new(
            r.x + r.w - 2.0 * bw - pad - pad / 2.0,
            top + (icon - bh) / 2.0,
            bw,
            bh,
        );
        let b2 = Rect::new(r.x + r.w - bw - pad, b1.y, bw, bh);
        if comp.out {
            if f.button(b1, &back) {
                f.actions.push(UiAction::Dismiss);
            }
        } else if comp.rest_secs == 0 && f.button(b1, &call) {
            f.actions.push(UiAction::Summon(index));
        }
        let asking = *confirm == Some(index);
        if f.button(b2, if asking { &sure } else { &release }) {
            if asking {
                f.actions.push(UiAction::ReleaseCompanion(index));
                *confirm = None;
            } else {
                *confirm = Some(index);
            }
        }
    }
    let hint = f.tr("ui.companions.hint");
    f.label_fit(
        Role::Text,
        s.small,
        (r.x + pad, r.y + r.h - lh - pad),
        r.w - 2.0 * pad,
        c.dim_text,
        &hint,
    );
    closed
}

/// The rows of a list that may be taller than `area`: which ones to draw,
/// from `*first`, as many whole rows as fit. The wheel over the area moves
/// the list a row at a time; a thin bar on the right says where the view is.
/// Rows never spill out of the window: the ones that do not fit are skipped.
pub(super) fn scroll_rows(
    f: &mut Frame<'_>,
    area: Rect,
    heights: &[f32],
    first: &mut usize,
) -> std::ops::Range<usize> {
    let n = heights.len();
    let total: f32 = heights.iter().sum();
    if total <= area.h + 0.5 {
        *first = 0;
        return 0..n;
    }
    // The last start that still fills the area.
    let mut last_first = n;
    let mut tail = 0.0;
    while last_first > 0 && tail + heights[last_first - 1] <= area.h + 0.5 {
        last_first -= 1;
        tail += heights[last_first];
    }
    if area.contains(f.input.mouse) && f.input.wheel != 0.0 {
        let step = f.input.wheel.abs().ceil().max(1.0) as usize;
        *first = if f.input.wheel > 0.0 {
            first.saturating_sub(step)
        } else {
            *first + step
        };
    }
    *first = (*first).min(last_first);
    let mut end = *first;
    let mut used = 0.0;
    while end < n && used + heights[end] <= area.h + 0.5 {
        used += heights[end];
        end += 1;
    }
    // Where the view is: a thumb on a thin track at the right edge.
    let pad = f.px(f.theme.sizes.padding);
    let track = Rect::new(area.x + area.w + pad / 2.0 - f.px(3.0), area.y, f.px(3.0), area.h);
    let mut dim = f.theme.colors.border;
    dim[3] *= 0.6;
    f.painter.rect(track, dim);
    let before: f32 = heights[..*first].iter().sum();
    let shown: f32 = heights[*first..end].iter().sum();
    let thumb = Rect::new(
        track.x,
        track.y + track.h * before / total,
        track.w,
        (track.h * shown / total).max(f.px(12.0)),
    );
    f.painter.rect(thumb, f.theme.colors.highlight);
    *first..end
}

/// A window frame: the title (with the key that opens it) and, when
/// `key` is given, a close button in the corner. Returns where the content
/// starts and whether the close button was clicked.
pub(super) fn window(f: &mut Frame<'_>, r: Rect, title_key: &'static str, key: Option<&str>) -> (f32, bool) {
    f.place(title_key, r);
    f.panel(r, true);
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let mut title = f.tr(title_key);
    if let Some(k) = key.filter(|k| !k.is_empty()) {
        title = format!("{title} ({k})");
    }
    let c = f.theme.colors.clone();
    let th = f.line_height(Role::Title, s.title);
    let mut closed = false;
    let mut room = r.w - 2.0 * pad;
    if key.is_some() {
        let side = th.min(f.px(22.0));
        let x = Rect::new(r.x + r.w - pad - side, r.y + pad + (th - side) / 2.0, side, side);
        let hover = x.contains(f.input.mouse);
        f.panel(x, hover);
        cross(
            f,
            x.shrink(side * 0.28),
            if hover { c.highlight } else { c.dim_text },
        );
        closed = hover && f.input.clicked;
        room -= side + pad;
    }
    f.label_fit(
        Role::Title,
        s.title,
        (r.x + pad, r.y + pad),
        room,
        c.highlight,
        &title,
    );
    (r.y + 2.0 * pad + th, closed)
}

/// An X drawn with two thin bars (no font needs the glyph).
pub(super) fn cross(f: &mut Frame<'_>, r: Rect, color: Color) {
    let t = f.px(1.5);
    for (a, b) in [
        ((r.x, r.y), (r.x + r.w, r.y + r.h)),
        ((r.x + r.w, r.y), (r.x, r.y + r.h)),
    ] {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let (nx, ny) = (-dy / len * t, dx / len * t);
        let (p0, p1, p2, p3) = (
            (a.0 + nx, a.1 + ny),
            (b.0 + nx, b.1 + ny),
            (b.0 - nx, b.1 - ny),
            (a.0 - nx, a.1 - ny),
        );
        f.painter.tri(p0, p1, p2, color);
        f.painter.tri(p0, p2, p3, color);
    }
}

const BAG_COLS: usize = 5;

/// How far down a window from `x` and `w` wide may reach: above the hotbar
/// when it stands over it (a narrow screen), else the bottom of the screen.
pub(super) fn above_hotbar(f: &Frame<'_>, x: f32, w: f32) -> f32 {
    let pad = f.px(f.theme.sizes.padding);
    let (first, last) = (hotbar_rect(f, 0), hotbar_rect(f, 9));
    if x < last.x + last.w && first.x < x + w {
        first.y - pad
    } else {
        f.input.screen.1 - pad
    }
}

pub(super) fn inventory_width(f: &Frame<'_>) -> f32 {
    let pad = f.px(f.theme.sizes.padding);
    BAG_COLS as f32 * (bag_slot(f) + pad / 2.0) + 1.5 * pad
}

/// Side of a backpack slot: the hotbar's, unless the backpack would then
/// stand over the hotbar (a narrow screen, 1024 wide): then a little
/// smaller, so it fits beside it and keeps the whole height.
fn bag_slot(f: &Frame<'_>) -> f32 {
    let s = &f.theme.sizes;
    let (pad, full) = (f.px(s.padding), f.px(s.hotbar_slot));
    let width = |slot: f32| BAG_COLS as f32 * (slot + pad / 2.0) + 1.5 * pad;
    let last = hotbar_rect(f, 9);
    let room = f.input.screen.0 - pad - (last.x + last.w + pad);
    if width(full) <= room {
        return full;
    }
    ((room - 1.5 * pad) / BAG_COLS as f32 - pad / 2.0).clamp(f.px(32.0), full)
}

pub(super) fn rarity_key(r: base::Rarity) -> &'static str {
    match r {
        base::Rarity::Common => "ui.rarity.common",
        base::Rarity::Rare => "ui.rarity.rare",
        base::Rarity::Epic => "ui.rarity.epic",
        base::Rarity::Unique => "ui.rarity.unique",
        base::Rarity::Legendary => "ui.rarity.legendary",
        base::Rarity::Mythic => "ui.rarity.mythic",
    }
}

/// A corner of the rarity's color on a slot (nothing for common items).
pub(super) fn rarity_corner(f: &mut Frame<'_>, r: Rect, rarity: base::Rarity) {
    if rarity == base::Rarity::Common {
        return;
    }
    let d = r.w * 0.3;
    let color = f.theme.colors.of_rarity(rarity);
    f.painter.tri(
        (r.x + r.w - d, r.y),
        (r.x + r.w, r.y),
        (r.x + r.w, r.y + d),
        color,
    );
}

/// The title line of an item tooltip and, past common, its rarity.
pub(super) fn item_title(f: &Frame<'_>, name: &str, rarity: base::Rarity) -> Vec<(String, Color)> {
    let color = f.theme.colors.of_rarity(rarity);
    let mut lines = vec![(name.to_owned(), color)];
    if rarity != base::Rarity::Common {
        lines.push((f.tr(rarity_key(rarity)), color));
    }
    lines
}

/// The backpack, the coins and what the character wears. Returns true
/// when its close button was clicked.
pub(super) fn inventory(
    f: &mut Frame<'_>,
    m: &HudModel,
    drag: &mut Option<Drag>,
    mode: ShopMode,
    rects: &mut Vec<Rect>,
) -> bool {
    let shop_open = m.shop.is_some();
    rects.clear();
    let trade_open = m.trade.as_ref().is_some_and(|t| t.open);
    let offered = |i: usize| {
        m.trade
            .as_ref()
            .is_some_and(|t| t.mine.iter().any(|l| usize::from(l.slot) == i))
    };
    let s = f.theme.sizes.clone();
    let (pad, slot) = (f.px(s.padding), bag_slot(f));
    let cols = BAG_COLS;
    let rows = m.bag.len().div_ceil(cols).max(1);
    let lh = f.line_height(Role::Text, s.small);
    let w = inventory_width(f);
    // Three full-width cards when the screen is tall enough; three cards
    // side by side when it is not (1024 x 600).
    let row_h = equip_row_height(f);
    let card_h = equip_card_height(f);
    let base = f.line_height(Role::Title, s.title) + rows as f32 * (slot + pad / 2.0) + 3.0 * lh + 5.5 * pad;
    let tall = base + 3.0 * (row_h + pad / 3.0);
    let x = f.input.screen.0 - w - pad;
    let room = above_hotbar(f, x, w) - right_top(f);
    let compact = tall > room;
    let h = if compact { base + card_h } else { tall };
    let r = Rect::new(x, right_top(f), w, h);
    let (mut y, closed) = window(
        f,
        r,
        "ui.inventory.title",
        Some(&key_of(m, MenuWindow::Inventory)),
    );
    let c = f.theme.colors.clone();
    let mut tooltip = None;
    for (i, b) in m.bag.iter().enumerate() {
        let sr = Rect::new(
            r.x + pad + (i % cols) as f32 * (slot + pad / 2.0),
            y + (i / cols) as f32 * (slot + pad / 2.0),
            slot,
            slot,
        );
        rects.push(sr);
        let hover = sr.contains(f.input.mouse);
        f.panel(sr, hover || b.as_ref().is_some_and(|b| b.unique));
        let Some(b) = b else { continue };
        // At a shop, what the mode cannot touch fades.
        let fits = match mode {
            ShopMode::Sell => b.can_sell,
            ShopMode::Repair => b.can_repair,
            ShopMode::Appraise => b.can_shop_appraise,
        };
        let tint = if shop_open && !trade_open && !fits {
            [1.0, 1.0, 1.0, 0.35]
        } else {
            [1.0; 4]
        };
        if m.pending.bag == Some(i as u16) || offered(i) {
            pulse(f, sr);
        }
        if drag.is_none() && f.drag_started(sr) {
            *drag = Some(Drag {
                from: DragSource::Bag(i as u16),
                icon: b.icon,
                label: b.name.clone(),
            });
        }
        let tap = drag.is_none() && f.tapped(sr);
        match b.icon {
            Some(uv) => f.painter.image(sr.shrink(pad / 4.0), uv, tint),
            None => f.label_fit(
                Role::Text,
                s.small,
                (sr.x + pad / 4.0, sr.y + pad / 4.0),
                slot - pad / 2.0,
                c.text,
                &b.name,
            ),
        }
        rarity_corner(f, sr, b.rarity);
        if b.count > 1 {
            let n = b.count.to_string();
            f.label(
                Role::Numbers,
                s.small,
                (sr.x + pad / 4.0, sr.y + slot - lh),
                c.highlight,
                &n,
            );
        }
        // Durability as a thin bar at the foot of the slot (phase 7).
        if let Some((cur, max)) = b.durability.filter(|(_, max)| *max > 0) {
            let share = (cur as f32 / max as f32).clamp(0.0, 1.0);
            let bar = Rect::new(
                sr.x + pad / 4.0,
                sr.y + slot - pad / 2.0,
                slot - pad / 2.0,
                pad / 4.0,
            );
            f.painter.rect(bar, c.dim_text);
            let color = if share <= 0.2 { c.error } else { c.secondary };
            f.painter
                .rect(Rect::new(bar.x, bar.y, bar.w * share, bar.h), color);
        }
        if hover {
            let mut lines = item_title(f, &b.name, b.rarity);
            if !b.description.is_empty() {
                lines.push((b.description.clone(), c.dim_text));
            }
            lines.extend(b.details.iter().map(|d| (d.clone(), c.text)));
            if let Some(a) = &b.appraised {
                lines.push((a.clone(), c.secondary));
            }
            // With a shop open a click does what the shop's mode says
            // (sell, repair or appraise), never two things at once.
            if trade_open {
                // An open trade takes the clicks: one offers the whole stack.
                if tap {
                    f.actions.push(UiAction::TradeAdd(i as u16));
                }
            } else if shop_open {
                let (key, color) = if fits {
                    (mode.click_key(), c.highlight)
                } else {
                    (mode.cannot_key(), c.dim_text)
                };
                lines.push((f.tr(key), color));
                if tap && fits {
                    f.actions.push(match mode {
                        // Shift sells the whole stack.
                        ShopMode::Sell => UiAction::Sell {
                            slot: i as u16,
                            qty: if f.input.shift { b.count.max(1) } else { 1 },
                        },
                        ShopMode::Repair => UiAction::Repair(i as u16),
                        ShopMode::Appraise => UiAction::ShopAppraise(i as u16),
                    });
                }
            } else {
                if b.unique && tap {
                    f.actions.push(UiAction::Equip(i as u16));
                }
                if b.usable && tap {
                    f.actions.push(UiAction::UseItem(i as u16));
                }
                // Right click appraises a piece not appraised yet and mends a
                // worn one; with both, Shift + right click mends.
                let mend = b.can_self_repair && (f.input.shift || !b.can_appraise);
                if b.can_appraise {
                    lines.push((f.tr("ui.item.right_appraise"), c.dim_text));
                    if f.input.right_clicked && !mend {
                        f.actions.push(UiAction::Appraise(i as u16));
                    }
                }
                if b.can_self_repair {
                    let key = if b.can_appraise {
                        "ui.item.shift_right_repair"
                    } else {
                        "ui.item.right_repair"
                    };
                    lines.push((f.tr(key), c.dim_text));
                    if f.input.right_clicked && mend {
                        f.actions.push(UiAction::SelfRepair(i as u16));
                    }
                }
            }
            tooltip = Some(lines);
        }
    }
    y += rows as f32 * (slot + pad / 2.0) + pad / 2.0;
    // How full the backpack is, at the right; the coins live in the status
    // strip, always in sight.
    let used = m.bag.iter().filter(|b| b.is_some()).count();
    let full = f.text.t_args(
        "ui.inventory.slots",
        &[Arg::Int(used as i64), Arg::Int(m.bag.len() as i64)],
    );
    let fw = f.fonts.measure(Role::Numbers, f.px(s.small), &full);
    let full_color = if used >= m.bag.len() { c.error } else { c.dim_text };
    f.label(
        Role::Numbers,
        s.small,
        (r.x + r.w - pad - fw, y),
        full_color,
        &full,
    );
    y += lh + pad / 2.0;
    // What the character wears: a header with a rule, then one card per slot
    // (icon, slot name, piece name, wear); a click takes it off.
    let header = f.tr("ui.inventory.equipment");
    let hw = f.fonts.measure(Role::Title, f.px(s.small), &header);
    f.label(Role::Title, s.small, (r.x + pad, y), c.highlight, &header);
    let rule_y = y + lh / 2.0;
    let rule_x = r.x + pad + hw + pad / 2.0;
    f.painter.rect(
        Rect::new(rule_x, rule_y, (r.x + r.w - pad - rule_x).max(0.0), 1.0),
        faded(c.border, 0.8),
    );
    y += lh + pad / 2.0;
    let gap = pad / 3.0;
    let card_w = (r.w - 2.0 * pad - 2.0 * gap) / 3.0;
    for (k, key) in [
        (0u8, "ui.inventory.weapon"),
        (1, "ui.inventory.armor"),
        (2, "ui.inventory.offhand"),
    ] {
        let worn = m.equipped.iter().find(|e| e.slot == k);
        let shown = if compact {
            let cr = Rect::new(r.x + pad + f32::from(k) * (card_w + gap), y, card_w, card_h);
            equip_card(f, cr, key, worn)
        } else {
            let rr = Rect::new(r.x + pad, y, r.w - 2.0 * pad, row_h);
            y += row_h + gap;
            equip_row(f, rr, key, worn)
        };
        if let Some(lines) = shown {
            tooltip = Some(lines);
            if f.input.clicked {
                f.actions.push(UiAction::Unequip(k));
            }
        }
    }
    let hint = if trade_open {
        f.tr("ui.inventory.trade_hint")
    } else if shop_open {
        f.tr(mode.hint_key())
    } else {
        f.tr("ui.inventory.hint")
    };
    f.label_fit(
        Role::Text,
        s.small,
        (r.x + pad, r.y + r.h - lh - pad),
        r.w - 2.0 * pad,
        c.dim_text,
        &hint,
    );
    if let Some(lines) = tooltip {
        overlay::tooltip(f, &lines);
    }
    closed
}

/// Height of one equipment card: the icon, or two lines of text and the wear
/// bar, whichever is taller.
fn equip_row_height(f: &Frame<'_>) -> f32 {
    let s = &f.theme.sizes;
    let (pad, slot) = (f.px(s.padding), f.px(s.hotbar_slot));
    let lh = f.line_height(Role::Text, s.small);
    slot.max(2.0 * lh + pad)
}

/// Height of one side-by-side card: the icon box, one line and the wear bar.
fn equip_card_height(f: &Frame<'_>) -> f32 {
    let s = &f.theme.sizes;
    let (pad, slot) = (f.px(s.padding), f.px(s.hotbar_slot));
    slot * 0.7 + f.line_height(Role::Text, s.small) + 3.0 + pad
}

/// The equipment card of a short screen: a rarity stripe on top, the icon
/// centered, under it the piece name (or the slot name while empty) and the
/// wear bar. The tooltip names the slot too.
fn equip_card(
    f: &mut Frame<'_>,
    cr: Rect,
    key: &str,
    worn: Option<&EquippedModel>,
) -> Option<Vec<(String, Color)>> {
    let s = f.theme.sizes.clone();
    let c = f.theme.colors.clone();
    let (pad, slot) = (f.px(s.padding), f.px(s.hotbar_slot));
    let lh = f.line_height(Role::Text, s.small);
    let hover = cr.contains(f.input.mouse);
    f.panel(cr, hover && worn.is_some());
    let accent = worn.map_or(faded(c.dim_text, 0.35), |e| c.of_rarity(e.rarity));
    f.painter
        .rect(Rect::new(cr.x + pad / 2.0, cr.y + 1.0, cr.w - pad, 2.0), accent);
    let side = slot * 0.7;
    let ir = Rect::new(cr.x + (cr.w - side) / 2.0, cr.y + pad / 4.0 + 2.0, side, side);
    sunken_box(f, ir, worn.map_or(c.border, |e| c.of_rarity(e.rarity)));
    let ty = ir.y + ir.h + pad / 4.0;
    let slot_name = f.tr(key);
    let Some(e) = worn else {
        let tw = f.fonts.measure(Role::Text, f.px(s.small), &slot_name);
        let tx = cr.x + ((cr.w - tw) / 2.0).max(pad / 4.0);
        f.label_fit(
            Role::Text,
            s.small,
            (tx, ty),
            cr.w - pad / 2.0,
            c.dim_text,
            &slot_name,
        );
        return None;
    };
    if let Some(uv) = e.icon {
        f.painter.image(ir.shrink(pad / 8.0), uv, [1.0; 4]);
    }
    rarity_corner(f, ir, e.rarity);
    let nw = f.fonts.measure(Role::Text, f.px(s.small), &e.name);
    let nx = cr.x + ((cr.w - nw) / 2.0).max(pad / 4.0);
    f.label_fit(
        Role::Text,
        s.small,
        (nx, ty),
        cr.w - pad / 2.0,
        c.of_rarity(e.rarity),
        &e.name,
    );
    if let Some((cur, max)) = e.durability {
        wear_bar(f, Rect::new(cr.x + pad / 2.0, ty + lh, cr.w - pad, 2.0), cur, max);
    }
    hover.then(|| {
        let mut lines = item_title(f, &e.name, e.rarity);
        lines.insert(1, (slot_name, c.dim_text));
        lines.extend(e.details.iter().map(|d| (d.clone(), c.text)));
        lines.push((f.tr("ui.inventory.click_unequip"), c.dim_text));
        lines
    })
}

/// A dark box with a thin border in `edge`, for an icon.
fn sunken_box(f: &mut Frame<'_>, ir: Rect, edge: Color) {
    let c = f.theme.colors.clone();
    f.painter.rect(ir, faded(c.background, 0.7));
    let edge = faded(edge, 0.6);
    for line in [
        Rect::new(ir.x, ir.y, ir.w, 1.0),
        Rect::new(ir.x, ir.y + ir.h - 1.0, ir.w, 1.0),
        Rect::new(ir.x, ir.y, 1.0, ir.h),
        Rect::new(ir.x + ir.w - 1.0, ir.y, 1.0, ir.h),
    ] {
        f.painter.rect(line, edge);
    }
}

/// Durability as a thin bar: red at a fifth or less.
fn wear_bar(f: &mut Frame<'_>, bar: Rect, cur: u32, max: u32) {
    let c = f.theme.colors.clone();
    let share = (cur as f32 / max.max(1) as f32).clamp(0.0, 1.0);
    let color = if share <= 0.2 { c.error } else { c.secondary };
    f.painter.rect(bar, faded(c.dim_text, 0.35));
    f.painter
        .rect(Rect::new(bar.x, bar.y, bar.w * share, bar.h), color);
}

/// The same color, more transparent.
fn faded(c: Color, k: f32) -> Color {
    [c[0], c[1], c[2], c[3] * k]
}

/// One equipment card: a rarity stripe, the icon in a sunken box, the slot
/// name over the piece name, and the wear as a thin bar along the foot.
/// Returns the tooltip when the mouse is over a worn piece.
fn equip_row(
    f: &mut Frame<'_>,
    rr: Rect,
    key: &str,
    worn: Option<&EquippedModel>,
) -> Option<Vec<(String, Color)>> {
    let s = f.theme.sizes.clone();
    let c = f.theme.colors.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.small);
    let hover = rr.contains(f.input.mouse);
    f.panel(rr, hover && worn.is_some());
    let accent = worn.map_or(faded(c.dim_text, 0.35), |e| c.of_rarity(e.rarity));
    f.painter.rect(
        Rect::new(rr.x + pad / 4.0, rr.y + pad / 2.0, pad / 4.0, rr.h - pad),
        accent,
    );
    // The icon box, sunken into the card.
    let side = rr.h - pad / 2.0;
    let ir = Rect::new(rr.x + pad, rr.y + pad / 4.0, side, side);
    sunken_box(f, ir, worn.map_or(c.border, |e| c.of_rarity(e.rarity)));
    let tx = ir.x + ir.w + pad * 0.75;
    let tw = (rr.x + rr.w - pad - tx).max(0.0);
    let ty = rr.y + (rr.h - 2.0 * lh - pad / 2.0) / 2.0;
    let slot_name = f.tr(key);
    f.label_fit(Role::Text, s.small, (tx, ty), tw, c.dim_text, &slot_name);
    let Some(e) = worn else {
        let empty = f.tr("ui.inventory.empty");
        f.label_fit(
            Role::Text,
            s.small,
            (tx, ty + lh),
            tw,
            faded(c.dim_text, 0.6),
            &empty,
        );
        return None;
    };
    if let Some(uv) = e.icon {
        f.painter.image(ir.shrink(pad / 4.0), uv, [1.0; 4]);
    }
    rarity_corner(f, ir, e.rarity);
    // Wear as numbers at the right of the slot name, and as a bar.
    if let Some((cur, max)) = e.durability {
        let n = format!("{cur}/{max}");
        let nw = f.fonts.measure(Role::Numbers, f.px(s.small), &n);
        if nw < tw / 2.0 {
            f.label(Role::Numbers, s.small, (tx + tw - nw, ty), c.dim_text, &n);
        }
        let bar = Rect::new(tx, ty + 2.0 * lh + pad / 8.0, tw, (pad / 4.0).max(2.0));
        wear_bar(f, bar, cur, max);
    }
    f.label_fit(
        Role::Text,
        s.small,
        (tx, ty + lh),
        tw,
        c.of_rarity(e.rarity),
        &e.name,
    );
    hover.then(|| {
        let mut lines = item_title(f, &e.name, e.rarity);
        lines.extend(e.details.iter().map(|d| (d.clone(), c.text)));
        lines.push((f.tr("ui.inventory.click_unequip"), c.dim_text));
        lines
    })
}

/// The open shop (phase 7): what it sells, with the price of one piece.
/// Selling, repairing and appraising happen in the backpack, beside it, by
/// the mode chosen in the tabs here (alpha).
pub(super) fn shop(
    f: &mut Frame<'_>,
    m: &ShopModel,
    pending: Option<u16>,
    mode: &mut ShopMode,
    first: &mut usize,
) -> bool {
    let s = f.theme.sizes.clone();
    let (pad, slot) = (f.px(s.padding), f.px(s.hotbar_slot));
    let lh = f.line_height(Role::Text, s.small);
    let w = f.px(320.0);
    let rows = m.lines.len().max(1) as f32;
    let modes: Vec<ShopMode> = [
        Some(ShopMode::Sell),
        m.repairs.then_some(ShopMode::Repair),
        m.appraises.then_some(ShopMode::Appraise),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !modes.contains(mode) {
        *mode = ShopMode::Sell;
    }
    let tabs_h = lh + pad;
    let purse_h = if m.purse.is_some() { lh } else { 0.0 };
    let h = f.line_height(Role::Title, s.title)
        + rows * (slot + pad / 2.0)
        + 4.0 * lh
        + 5.0 * pad
        + tabs_h
        + purse_h;
    // Left of the inventory, which a shop keeps open.
    let top = right_top(f);
    let r = Rect::new(
        (f.input.screen.0 - inventory_width(f) - w - 2.0 * pad).max(pad),
        top,
        w,
        h.min(f.input.screen.1 - top - f.px(90.0)),
    );
    let (mut y, closed) = window(f, r, "ui.shop.title", Some(""));
    let c = f.theme.colors.clone();
    f.label_fit(
        Role::Text,
        s.small,
        (r.x + pad, y),
        r.w - 2.0 * pad,
        c.dim_text,
        &m.title,
    );
    // What the player can spend, on the same line at the right.
    let cw = f.fonts.measure(Role::Numbers, f.px(s.small), &m.coins);
    f.label(
        Role::Numbers,
        s.small,
        (r.x + r.w - pad - cw, y),
        c.highlight,
        &m.coins,
    );
    y += lh + pad / 2.0;
    // What a click on the backpack does: sell, repair or appraise.
    let n = modes.len() as f32;
    let each = (r.w - 2.0 * pad - (n - 1.0) * pad / 4.0) / n;
    for (i, md) in modes.iter().enumerate() {
        let tr = Rect::new(r.x + pad + i as f32 * (each + pad / 4.0), y, each, lh + pad / 2.0);
        let hover = tr.contains(f.input.mouse);
        let on = *md == *mode;
        f.panel(tr, hover || on);
        let color = if on { c.highlight } else { c.dim_text };
        let label = f.tr(md.key());
        f.label_center(Role::Text, s.small, tr, color, &label);
        if hover && f.input.clicked {
            *mode = *md;
        }
    }
    y += tabs_h;
    if let Some(p) = &m.purse {
        f.label_fit(
            Role::Text,
            s.small,
            (r.x + pad, y),
            r.w - 2.0 * pad,
            c.dim_text,
            p,
        );
        y += purse_h;
    }
    let mut tooltip = None;
    // The hint and the close line stay at the bottom; the stock scrolls.
    let foot = 2.0 * lh + pad;
    let area = Rect::new(r.x + pad, y, r.w - 2.0 * pad, r.y + r.h - pad - foot - y);
    let heights = vec![slot + pad / 2.0; m.lines.len()];
    let shown = scroll_rows(f, area, &heights, first);
    for (i, line) in m.lines.iter().enumerate().skip(shown.start).take(shown.len()) {
        let lr = Rect::new(area.x, y, area.w - pad / 2.0, slot);
        let hover = lr.contains(f.input.mouse);
        f.panel(lr, hover);
        if pending == Some(i as u16) {
            pulse(f, lr);
        }
        let ir = Rect::new(
            lr.x + pad / 4.0,
            lr.y + pad / 4.0,
            slot - pad / 2.0,
            slot - pad / 2.0,
        );
        if let Some(uv) = line.icon {
            f.painter.image(ir, uv, [1.0; 4]);
        }
        rarity_corner(f, ir, line.rarity);
        let text_x = lr.x + slot + pad / 2.0;
        let mut color = c.of_rarity(line.rarity);
        if !line.affordable {
            color[3] *= 0.55;
        }
        f.label_fit(
            Role::Text,
            s.small,
            (text_x, lr.y + pad / 4.0),
            lr.w - slot - pad,
            color,
            &line.name,
        );
        let price = match line.qty {
            Some(q) => f.text.t_args(
                "ui.shop.price_stock",
                &[Arg::Int(line.price as i64), Arg::Int(i64::from(q))],
            ),
            None => f.text.t_args("ui.shop.price", &[Arg::Int(line.price as i64)]),
        };
        f.label_fit(
            Role::Numbers,
            s.small,
            (text_x, lr.y + pad / 4.0 + lh),
            lr.w - slot - pad,
            if line.affordable { c.highlight } else { c.error },
            &price,
        );
        if hover {
            let mut lines = item_title(f, &line.name, line.rarity);
            lines.extend(line.tooltip.iter().map(|t| (t.clone(), c.text)));
            tooltip = Some(lines);
            if f.input.clicked && line.affordable && line.qty != Some(0) {
                // Shift buys five (or what is left).
                let want = if f.input.shift { 5 } else { 1 };
                let qty = line.qty.map_or(want, |q| q.min(want));
                f.actions.push(UiAction::Buy { index: i as u16, qty });
            }
        }
        y += slot + pad / 2.0;
    }
    y = r.y + r.h - pad - foot + pad / 2.0;
    let hint = f.tr(mode.hint_key());
    for text in [&hint, &m.close] {
        f.label_fit(
            Role::Text,
            s.small,
            (r.x + pad, y),
            r.w - 2.0 * pad,
            c.dim_text,
            text,
        );
        y += lh;
    }
    if let Some(lines) = tooltip {
        overlay::tooltip(f, &lines);
    }
    closed
}

pub(super) fn sheet(f: &mut Frame<'_>, m: &HudModel, first: &mut usize) -> bool {
    /// One line of the sheet.
    enum Row {
        Text(String, Color, f32),
        Attr(usize),
        Header(String),
        Class(usize),
        Title(usize),
    }
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.text);
    let small = f.line_height(Role::Text, s.small);
    let sm = &m.sheet;
    let c = f.theme.colors.clone();
    let mut rows = vec![
        Row::Text(
            sm.class.clone().unwrap_or_else(|| f.tr("ui.sheet.no_class")),
            c.highlight,
            s.text,
        ),
        Row::Text(
            f.text.t_args("ui.sheet.rank", &[Arg::Str(sm.rank.clone())]),
            c.text,
            s.text,
        ),
        Row::Text(
            f.text
                .t_args("ui.sheet.defense", &[Arg::Int(i64::from(sm.defense))]),
            c.text,
            s.text,
        ),
    ];
    if sm.free_points > 0 {
        let pts = f
            .text
            .t_args("ui.sheet.free_points", &[Arg::Int(i64::from(sm.free_points))]);
        rows.push(Row::Text(pts, c.system, s.text));
    }
    rows.extend((0..ATTRS.len()).map(Row::Attr));
    if !sm.specials.is_empty() {
        rows.push(Row::Header(f.tr("ui.sheet.specials")));
        rows.extend(
            sm.specials
                .iter()
                .map(|(name, v)| Row::Text(format!("{name} {v}"), c.text, s.small)),
        );
    }
    if !sm.classes_available.is_empty() {
        rows.push(Row::Header(f.tr("ui.sheet.classes")));
        rows.extend((0..sm.classes_available.len()).map(Row::Class));
    }
    if !sm.proficiencies.is_empty() {
        rows.push(Row::Header(f.tr("ui.sheet.proficiencies")));
        rows.extend(
            sm.proficiencies
                .iter()
                .map(|(name, v)| Row::Text(format!("{name} {v}"), c.text, s.small)),
        );
    }
    if !sm.titles.is_empty() {
        rows.push(Row::Header(f.tr("ui.sheet.titles")));
        rows.extend((0..sm.titles.len()).map(Row::Title));
    }
    let heights: Vec<f32> = rows
        .iter()
        .map(|r| match r {
            Row::Text(_, _, size) if *size == s.small => small,
            Row::Title(_) => small,
            Row::Class(_) => lh + pad / 4.0,
            _ => lh,
        })
        .collect();
    let w = f.px(320.0);
    let h = f.line_height(Role::Title, s.title) + heights.iter().sum::<f32>() + 4.0 * pad;
    let top = under_status(f);
    let r = Rect::new(pad, top, w, h.min(f.input.screen.1 - top - f.px(190.0)));
    let (mut y, closed) = window(f, r, "ui.sheet.title", Some(&key_of(m, MenuWindow::Sheet)));
    let area = Rect::new(r.x + pad, y, r.w - 2.0 * pad, r.y + r.h - pad - y);
    let inner = area.w - pad / 2.0;
    let mut tip = None;
    for i in scroll_rows(f, area, &heights, first) {
        match &rows[i] {
            Row::Text(text, color, size) => {
                f.label_fit(Role::Text, *size, (area.x, y), inner, *color, text);
            }
            Row::Header(text) => {
                f.label_fit(Role::Title, s.text, (area.x, y), inner, c.system, text);
            }
            Row::Attr(a) => {
                let name = f.tr(&format!("ui.attr.{}", ATTRS[*a]));
                // What the attribute does, on hover.
                if Rect::new(area.x, y, inner - lh - pad, lh).contains(f.input.mouse) {
                    let help = f.tr(&format!("ui.attr_help.{}", ATTRS[*a]));
                    tip = Some(vec![(name.clone(), c.highlight), (help, c.text)]);
                }
                f.label_fit(Role::Text, s.text, (area.x, y), inner * 0.55, c.text, &name);
                // The sheet's own value, and what equipment and titles add.
                let (total, base) = (sm.attributes[*a], sm.base_attributes[*a]);
                let v = base.min(total).to_string();
                let vw = f.label(Role::Numbers, s.text, (area.x + inner * 0.6, y), c.text, &v);
                if total > base {
                    let more = format!(" (+{})", total - base);
                    f.label(
                        Role::Numbers,
                        s.text,
                        (area.x + inner * 0.6 + vw, y),
                        c.secondary,
                        &more,
                    );
                    if Rect::new(area.x, y, inner - lh - pad, lh).contains(f.input.mouse)
                        && let Some(t) = tip.as_mut()
                    {
                        t.push((f.tr("ui.attr_help.bonus"), c.secondary));
                    }
                }
                if sm.free_points > 0 && f.button(Rect::new(area.x + inner - lh, y, lh, lh), "+") {
                    f.actions.push(UiAction::AddPoint(*a));
                }
            }
            Row::Title(k) => {
                let name = &sm.titles[*k];
                f.label_fit(Role::Text, s.small, (area.x, y), inner, c.secondary, name);
                // What the title gives, on hover.
                if Rect::new(area.x, y, inner, small).contains(f.input.mouse) {
                    let mut lines = vec![(name.clone(), c.highlight)];
                    if let Some(more) = sm.title_tips.get(*k) {
                        lines.extend(more.iter().map(|l| (l.clone(), c.text)));
                    }
                    tip = Some(lines);
                }
            }
            Row::Class(k) => {
                let name = sm.classes_available[*k].clone();
                if f.button(Rect::new(area.x, y, inner, lh), &name) {
                    f.actions.push(UiAction::ChooseClass(*k));
                }
            }
        }
        y += heights[i];
    }
    if let Some(lines) = tip {
        overlay::tooltip(f, &lines);
    }
    closed
}

/// The quest log, on the right (left of the inventory when it is open).
pub(super) fn quests(f: &mut Frame<'_>, m: &HudModel, beside_inventory: bool, first: &mut usize) -> bool {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.small);
    let w = f.px(340.0).min(f.input.screen.0 - 2.0 * pad);
    let px = f.px(s.small);
    let inner = w - 3.0 * pad;
    // Lines first, to size the window.
    let mut rows: Vec<(String, crate::Color, bool)> = Vec::new();
    let c = f.theme.colors.clone();
    if m.quests.is_empty() {
        let empty = f.tr("ui.quests.empty");
        for l in f.fonts.wrap(Role::Text, px, &empty, inner) {
            rows.push((l, c.dim_text, false));
        }
    }
    for q in &m.quests {
        if q.completed {
            let line = f.text.t_args("ui.quests.done", &[Arg::Str(q.name.clone())]);
            rows.push((line, c.dim_text, false));
            continue;
        }
        rows.push((q.name.clone(), c.highlight, false));
        for l in f.fonts.wrap(Role::Text, px, &q.description, inner) {
            rows.push((l, c.dim_text, true));
        }
        for (o, done) in &q.objectives {
            let mark = if *done { "+" } else { "-" };
            let color = if *done { c.dim_text } else { c.text };
            rows.push((format!("{mark} {o}"), color, true));
        }
    }
    let title_h = f.line_height(Role::Title, s.title);
    let mut x = f.input.screen.0 - w - pad;
    if beside_inventory {
        x -= inventory_width(f) + pad;
    }
    let x = x.max(pad);
    let h = (title_h + rows.len() as f32 * lh + 4.0 * pad).min(above_hotbar(f, x, w) - right_top(f));
    let r = Rect::new(x, right_top(f), w, h);
    let (mut y, closed) = window(f, r, "ui.quests.title", Some(&key_of(m, MenuWindow::Quests)));
    let area = Rect::new(r.x + pad, y, r.w - 2.0 * pad, r.y + r.h - pad - y);
    let heights = vec![lh; rows.len()];
    for (text, color, indent) in &rows[scroll_rows(f, area, &heights, first)] {
        let dx = if *indent { pad } else { 0.0 };
        f.label_fit(
            Role::Text,
            s.small,
            (area.x + dx, y),
            area.w - pad / 2.0 - dx,
            *color,
            text,
        );
        y += lh;
    }
    closed
}

/// The open quests in a corner while the quest log is closed: the name and
/// what is still to do. A click opens the log (returns true).
pub(super) fn tracker(f: &mut Frame<'_>, m: &HudModel) -> bool {
    let open: Vec<&QuestModel> = m.quests.iter().filter(|q| !q.completed).take(3).collect();
    if open.is_empty() {
        return false;
    }
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.small);
    let c = f.theme.colors.clone();
    let mut rows: Vec<(String, Color, bool)> = Vec::new();
    for q in &open {
        rows.push((q.name.clone(), c.highlight, false));
        for (o, done) in &q.objectives {
            let mark = if *done { "+" } else { "-" };
            rows.push((
                format!("{mark} {o}"),
                if *done { c.dim_text } else { c.text },
                true,
            ));
        }
        if let Some(g) = &q.goal {
            rows.push((format!("> {g}"), c.system, true));
        }
    }
    let w = f.px(250.0).min(f.input.screen.0 / 3.0);
    let top = right_top(f);
    let h = (rows.len() as f32 * lh + 2.0 * pad)
        .min(f.input.screen.1 / 2.0 - top)
        .max(lh + 2.0 * pad);
    let r = Rect::new(f.input.screen.0 - w - pad, top, w, h);
    f.place("tracker", r);
    let hover = r.contains(f.input.mouse);
    f.panel(r, hover);
    let fit = ((r.h - 2.0 * pad) / lh).floor().max(1.0) as usize;
    for (i, (text, color, indent)) in rows.iter().take(fit).enumerate() {
        let dx = if *indent { pad } else { 0.0 };
        f.label_fit(
            Role::Text,
            s.small,
            (r.x + pad + dx, r.y + pad + i as f32 * lh),
            r.w - 2.0 * pad - dx,
            *color,
            text,
        );
    }
    if hover {
        let key = key_of(m, MenuWindow::Quests);
        let tip = if key.is_empty() {
            f.tr("ui.quests.tracker")
        } else {
            format!("{} ({key})", f.tr("ui.quests.tracker"))
        };
        overlay::tooltip(f, &[(tip, c.text)]);
    }
    hover && f.input.clicked
}

/// The keys and a few tips (F1), in the middle of the screen.
/// The Options window: the volume of the whole and of each
/// channel in 10% steps, mute and a test sound. Returns true when closed.
pub(super) fn options(f: &mut Frame<'_>, m: &HudModel) -> bool {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.text);
    let row = lh + pad / 2.0;
    let c = f.theme.colors.clone();
    let w = f.px(380.0).min(f.input.screen.0 - 2.0 * pad);
    let title_h = f.line_height(Role::Title, s.title);
    let notice = usize::from(!m.sound.live);
    let rows = 1 + m.sound.volumes.len() + 1 + notice;
    let h = title_h + rows as f32 * row + 4.0 * pad;
    let top = under_status(f) + f.px(40.0);
    let r = Rect::new((f.input.screen.0 - w) / 2.0, top, w, h);
    let (mut y, closed) = window(f, r, "ui.options.title", Some(&key_of(m, MenuWindow::Options)));
    let x = r.x + pad;
    let inner = r.w - 2.0 * pad;
    let header = f.tr("ui.options.sound");
    f.label_fit(Role::Title, s.small, (x, y), inner, c.system, &header);
    y += row;
    let button = lh * 1.3;
    let value_w = f.px(52.0);
    for &(slot, per_mille) in &m.sound.volumes {
        let label = f.tr(slot.label_key());
        let color = if m.sound.muted { c.dim_text } else { c.text };
        let label_w = inner - 2.0 * button - value_w - pad;
        f.label_fit(Role::Text, s.text, (x, y), label_w, color, &label);
        let minus = Rect::new(x + inner - 2.0 * button - value_w, y, button, lh);
        let plus = Rect::new(x + inner - button, y, button, lh);
        if f.button(minus, "−") && per_mille > 0 {
            f.actions.push(UiAction::SoundStep { slot, up: false });
        }
        let pct = format!("{}%", per_mille / 10);
        let pw = f.fonts.measure(Role::Numbers, f.px(s.text), &pct);
        let vx = minus.x + button + (value_w - pw) / 2.0;
        f.label(Role::Numbers, s.text, (vx, y), color, &pct);
        if f.button(plus, "+") && per_mille < 1000 {
            f.actions.push(UiAction::SoundStep { slot, up: true });
        }
        y += row;
    }
    let half = (inner - pad) / 2.0;
    let mute_key = if m.sound.muted {
        "ui.options.unmute"
    } else {
        "ui.options.mute"
    };
    let mute = f.tr(mute_key);
    if f.button(Rect::new(x, y, half, lh), &mute) {
        f.actions.push(UiAction::SoundMute);
    }
    let test = f.tr("ui.options.test");
    if f.button(Rect::new(x + half + pad, y, half, lh), &test) {
        f.actions.push(UiAction::SoundTest);
    }
    y += row;
    if !m.sound.live {
        let none = f.tr("ui.options.no_sound");
        f.label_fit(Role::Text, s.small, (x, y), inner, c.dim_text, &none);
    }
    closed
}

pub(super) fn help(f: &mut Frame<'_>, m: &HudModel, first: &mut usize) -> bool {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.small);
    let w = f.px(460.0).min(f.input.screen.0 - 2.0 * pad);
    let inner = w - 3.0 * pad;
    let c = f.theme.colors.clone();
    // `(left, right, color)`: a key row, a header or a wrapped tip.
    let mut rows: Vec<(String, String, Color)> = vec![(f.tr("ui.help.keys"), String::new(), c.system)];
    rows.extend(
        m.help
            .keys
            .iter()
            .map(|(what, key)| (what.clone(), key.clone(), c.text)),
    );
    rows.push((f.tr("ui.help.tips"), String::new(), c.system));
    let px = f.px(s.small);
    for t in &m.help.tips {
        for l in f.fonts.wrap(Role::Text, px, t, inner) {
            rows.push((l, String::new(), c.dim_text));
        }
    }
    let title_h = f.line_height(Role::Title, s.title);
    let top = under_status(f) + f.px(40.0);
    let h = (title_h + rows.len() as f32 * lh + 4.0 * pad).min(f.input.screen.1 - top - f.px(120.0));
    let r = Rect::new((f.input.screen.0 - w) / 2.0, top, w, h);
    let (mut y, closed) = window(f, r, "ui.help.title", Some(&key_of(m, MenuWindow::Help)));
    let area = Rect::new(r.x + pad, y, r.w - 2.0 * pad, r.y + r.h - pad - y);
    let heights = vec![lh; rows.len()];
    for (left, right, color) in &rows[scroll_rows(f, area, &heights, first)] {
        let header = right.is_empty() && *color == c.system;
        let role = if header { Role::Title } else { Role::Text };
        let lw = if right.is_empty() {
            area.w - pad / 2.0
        } else {
            area.w * 0.7
        };
        f.label_fit(role, s.small, (area.x, y), lw, *color, left);
        if !right.is_empty() {
            let kw = f.fonts.measure(Role::Numbers, px, right);
            f.label(
                Role::Numbers,
                s.small,
                (area.x + area.w - pad - kw, y),
                c.highlight,
                right,
            );
        }
        y += lh;
    }
    closed
}

/// Where the dialogue window goes, and its text wrapped to fit.
pub(super) fn dialogue_rect(f: &mut Frame<'_>, text: &str, options: &[String]) -> (Rect, Vec<String>) {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.text);
    let w = f.px(520.0).min(f.input.screen.0 - 2.0 * pad);
    let px = f.px(s.text);
    let lines = f.fonts.wrap(Role::Text, px, text, w - 2.0 * pad);
    let small = f.line_height(Role::Text, s.small);
    let h = (lines.len() + options.len()) as f32 * (lh + pad / 2.0) + 3.0 * pad + small;
    let r = Rect::new(
        (f.input.screen.0 - w) / 2.0,
        f.input.screen.1 * 0.55 - h / 2.0,
        w,
        h,
    );
    (r, lines)
}

pub(super) fn dialogue(f: &mut Frame<'_>, text: &str, options: &[String], close: &str) {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.text);
    let small = f.line_height(Role::Text, s.small);
    let (r, lines) = dialogue_rect(f, text, options);
    f.place("dialogue", r);
    f.panel(r, true);
    let c = f.theme.colors.clone();
    let mut y = r.y + pad;
    for l in &lines {
        f.label(Role::Text, s.text, (r.x + pad, y), c.text, l);
        y += lh;
    }
    y += pad;
    for (i, o) in options.iter().enumerate() {
        let label = format!("{}. {o}", i + 1);
        if f.button(Rect::new(r.x + pad, y, r.w - 2.0 * pad, lh + pad / 4.0), &label) {
            f.actions.push(UiAction::ChooseOption(i as u16));
        }
        y += lh + pad / 2.0;
    }
    let dim = c.dim_text;
    let px = f.px(s.small);
    let w = f.fonts.measure(Role::Text, px, close);
    f.label(
        Role::Text,
        s.small,
        (r.x + r.w - pad - w, r.y + r.h - pad - small),
        dim,
        close,
    );
}

pub(super) fn fallen(f: &mut Frame<'_>) {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Title, s.title);
    let w = f.px(360.0);
    let r = Rect::new(
        (f.input.screen.0 - w) / 2.0,
        f.input.screen.1 * 0.4,
        w,
        2.0 * lh + 4.0 * pad,
    );
    f.place("fallen", r);
    f.panel(r, true);
    let text = f.tr("ui.hud.fallen");
    let life = f.theme.colors.life;
    f.label_center(
        Role::Title,
        s.title,
        Rect::new(r.x, r.y + pad, r.w, lh),
        life,
        &text,
    );
    let label = f.tr("ui.hud.give_up");
    if f.button(
        Rect::new(r.x + r.w / 4.0, r.y + 2.0 * pad + lh, r.w / 2.0, lh + pad),
        &label,
    ) {
        f.actions.push(UiAction::GiveUp);
    }
}
