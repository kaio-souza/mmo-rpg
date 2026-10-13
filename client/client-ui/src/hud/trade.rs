//! The trade window and the invite panel: both offers side by
//! side, piece by piece, with copper, Confirm and Cancel.

use super::*;

/// Focus id of the copper field.
const COINS_FIELD: u32 = 300;
/// Copper a player can type: twelve digits.
const COINS_CHARS: usize = 12;
/// Focus id of the quantity being typed on one of my lines.
const QTY_FIELD: u32 = 301;

/// My offer with one line's quantity changed.
fn with_qty(t: &TradeModel, slot: u16, qty: u32) -> UiAction {
    let items = t
        .mine
        .iter()
        .map(|l| (l.slot, if l.slot == slot { qty } else { l.qty }))
        .collect();
    UiAction::TradeOffer {
        items,
        coins: t.mine_coins,
    }
}

/// How many of a stack go: `-` and `+`, the number (click to type it,
/// Enter puts it down) and the wheel over the line. Returns the width used
/// at the right of the line and whether the mouse is on the controls.
fn qty_controls(
    f: &mut Frame<'_>,
    t: &TradeModel,
    line: &TradeLine,
    lr: Rect,
    edit: &mut Option<(u16, String)>,
) -> (f32, bool) {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let h = f.line_height(Role::Text, s.small) + pad / 2.0;
    let (bw, fw) = (h, f.px(40.0));
    let w = 2.0 * bw + fw + pad / 2.0;
    let y = lr.y + (lr.h - h) / 2.0;
    let minus = Rect::new(lr.x + lr.w - w - pad / 4.0, y, bw, h);
    let field = Rect::new(minus.x + bw + pad / 4.0, y, fw, h);
    let plus = Rect::new(field.x + fw + pad / 4.0, y, bw, h);
    let on = Rect::new(minus.x, y, w, h).contains(f.input.mouse);
    let set = |f: &mut Frame<'_>, n: u32| {
        let n = n.clamp(1, line.max.max(1));
        if n != line.qty {
            f.actions.push(with_qty(t, line.slot, n));
        }
    };
    if f.button(minus, "-") {
        set(f, line.qty.saturating_sub(1));
    }
    if f.button(plus, "+") {
        set(f, line.qty.saturating_add(1));
    }
    if lr.contains(f.input.mouse) && f.input.wheel != 0.0 {
        let step = f.input.wheel.abs().ceil().max(1.0) as u32;
        let n = if f.input.wheel > 0.0 {
            line.qty.saturating_add(step)
        } else {
            line.qty.saturating_sub(step)
        };
        set(f, n);
    }
    let editing = edit.as_ref().is_some_and(|(slot, _)| *slot == line.slot);
    let left = f.input.clicked && !field.contains(f.input.mouse);
    if editing && (left || *f.focus != Some(QTY_FIELD)) {
        if *f.focus == Some(QTY_FIELD) {
            *f.focus = None;
        }
        *edit = None;
    } else if editing {
        if let Some((_, draft)) = edit.as_mut() {
            draft.retain(|ch| ch.is_ascii_digit());
            let entered = f.field(QTY_FIELD, field, draft, false, 6);
            if entered {
                if let Ok(n) = draft.parse::<u32>() {
                    set(f, n);
                }
                *edit = None;
                *f.focus = None;
            }
        }
        return (w + pad / 2.0, on);
    }
    let hover = field.contains(f.input.mouse);
    f.panel(field, hover);
    let c = f.theme.colors.clone();
    f.label_center(Role::Numbers, s.small, field, c.highlight, &line.qty.to_string());
    if hover && f.input.clicked {
        *edit = Some((line.slot, String::new()));
        *f.focus = Some(QTY_FIELD);
    }
    (w + pad / 2.0, on)
}

/// Draws the trade; returns its rectangle (a backpack slot dropped on it
/// joins the offer).
pub(super) fn draw(
    f: &mut Frame<'_>,
    t: &TradeModel,
    coins: &mut String,
    qty_edit: &mut Option<(u16, String)>,
) -> Rect {
    let s = f.theme.sizes.clone();
    let (pad, slot) = (f.px(s.padding), f.px(s.hotbar_slot));
    let lh = f.line_height(Role::Text, s.small);
    let bh = f.line_height(Role::Title, s.text) + pad;
    let w = f.px(460.0).min(f.input.screen.0 - inventory_width(f) - 3.0 * pad);
    let rows = t.mine.len().max(t.theirs.len()).max(1) as f32;
    let body = if t.open {
        lh + rows * (slot + pad / 2.0) + 2.0 * lh + bh + 3.0 * pad
    } else {
        lh + bh + 2.0 * pad
    };
    let h = f.line_height(Role::Title, s.title) + body + 2.0 * pad;
    let top = right_top(f);
    // Left of the backpack, which the trade keeps open.
    let r = Rect::new(
        (f.input.screen.0 - inventory_width(f) - w - 2.0 * pad).max(pad),
        top,
        w,
        h.min(f.input.screen.1 - top - f.px(90.0)),
    );
    let (mut y, _) = window(f, r, "ui.trade.title", None);
    let c = f.theme.colors.clone();
    let inner = r.w - 2.0 * pad;
    if !t.open && t.asked {
        // They asked me: accept opens the trade, decline ends it.
        let ask = f
            .text
            .t_args("ui.social.invite_trade", &[Arg::Str(t.with.clone())]);
        f.label_fit(Role::Text, s.small, (r.x + pad, y), inner, c.highlight, &ask);
        y += lh + pad;
        let bw = (inner - pad) / 2.0;
        if f.button(Rect::new(r.x + pad, y, bw, bh), &f.tr("ui.trade.accept")) {
            f.actions.push(UiAction::TradeAccept);
        }
        if f.button(
            Rect::new(r.x + 2.0 * pad + bw, y, bw, bh),
            &f.tr("ui.trade.decline"),
        ) {
            f.actions.push(UiAction::TradeCancel);
        }
        return r;
    }
    if !t.open {
        let wait = f.text.t_args("ui.social.trade_wait", &[Arg::Str(t.with.clone())]);
        f.label_fit(Role::Text, s.small, (r.x + pad, y), inner, c.dim_text, &wait);
        y += lh + pad;
        if f.button(Rect::new(r.x + pad, y, inner, bh), &f.tr("ui.trade.cancel")) {
            f.actions.push(UiAction::TradeCancel);
        }
        return r;
    }
    let half = (inner - pad) / 2.0;
    let mut tooltip = None;
    let you = f.tr("ui.trade.you");
    for (k, (title, lines, gold, confirmed)) in [
        (you, &t.mine, t.mine_coins, t.mine_confirmed),
        (t.with.clone(), &t.theirs, t.theirs_coins, t.theirs_confirmed),
    ]
    .into_iter()
    .enumerate()
    {
        let x = r.x + pad + k as f32 * (half + pad);
        let mine = k == 0;
        // The side's name, and a mark once it confirmed.
        let head = if confirmed {
            f.text.t_args("ui.trade.confirmed", &[Arg::Str(title)])
        } else {
            title
        };
        let color = if confirmed { c.highlight } else { c.text };
        f.label_fit(Role::Text, s.small, (x, y), half, color, &head);
        let mut ly = y + lh;
        if lines.is_empty() {
            let hint = if mine {
                f.tr("ui.trade.drop_here")
            } else {
                f.tr("ui.social.nothing")
            };
            f.label_fit(Role::Text, s.small, (x, ly), half, c.dim_text, &hint);
        }
        for line in lines {
            let lr = Rect::new(x, ly, half, slot);
            let hover = lr.contains(f.input.mouse);
            f.panel(lr, hover);
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
            // A stack of mine: how many go is set right on the line.
            let (ctrl_w, on_ctrl) = if mine && line.max > 1 {
                qty_controls(f, t, line, lr, qty_edit)
            } else {
                (0.0, false)
            };
            let name = if line.qty > 1 && ctrl_w == 0.0 {
                format!("{} x{}", line.name, line.qty)
            } else {
                line.name.clone()
            };
            let tx = lr.x + slot + pad / 2.0;
            f.label_fit(
                Role::Text,
                s.small,
                (tx, lr.y + (slot - lh) / 2.0),
                lr.w - slot - pad - ctrl_w,
                c.of_rarity(line.rarity),
                &name,
            );
            if hover {
                let mut tip = item_title(f, &line.name, line.rarity);
                tip.extend(line.details.iter().map(|d| (d.clone(), c.text)));
                if mine {
                    if line.max > 1 {
                        tip.push((f.tr("ui.trade.qty_hint"), c.dim_text));
                    }
                    tip.push((f.tr("ui.trade.click_remove"), c.dim_text));
                    if f.input.clicked && !on_ctrl {
                        let items = t
                            .mine
                            .iter()
                            .filter(|l| l.slot != line.slot)
                            .map(|l| (l.slot, l.qty))
                            .collect();
                        f.actions.push(UiAction::TradeOffer {
                            items,
                            coins: t.mine_coins,
                        });
                    }
                }
                tooltip = Some(tip);
            }
            ly += slot + pad / 2.0;
        }
        let gy = y + lh + rows * (slot + pad / 2.0) + pad / 2.0;
        let copper = f.text.t_args(
            "ui.social.coins",
            &[Arg::Int(i64::try_from(gold).unwrap_or(i64::MAX))],
        );
        f.label(Role::Numbers, s.small, (x, gy), c.highlight, &copper);
        if mine {
            // Type copper and press Enter (or the button) to put it down.
            let fr = Rect::new(x, gy + lh, half * 0.6, lh + pad / 2.0);
            coins.retain(|ch| ch.is_ascii_digit());
            let entered = f.field(COINS_FIELD, fr, coins, false, COINS_CHARS);
            let br = Rect::new(fr.x + fr.w + pad / 2.0, fr.y, half - fr.w - pad / 2.0, fr.h);
            let set = f.button(br, &f.tr("ui.trade.set_coins"));
            if (entered || set)
                && let Ok(n) = coins.parse::<u64>()
            {
                let items = t.mine.iter().map(|l| (l.slot, l.qty)).collect();
                f.actions.push(UiAction::TradeOffer { items, coins: n });
                coins.clear();
            }
        }
    }
    y += lh + rows * (slot + pad / 2.0) + 2.0 * lh + 2.5 * pad;
    let bw = (inner - pad) / 2.0;
    let confirm = if t.mine_confirmed {
        f.tr("ui.trade.waiting")
    } else {
        f.tr("ui.trade.confirm")
    };
    if f.button(Rect::new(r.x + pad, y, bw, bh), &confirm) && !t.mine_confirmed {
        f.actions.push(UiAction::TradeConfirm);
    }
    if f.button(
        Rect::new(r.x + 2.0 * pad + bw, y, bw, bh),
        &f.tr("ui.trade.cancel"),
    ) {
        f.actions.push(UiAction::TradeCancel);
    }
    if let Some(lines) = tooltip {
        overlay::tooltip(f, &lines);
    }
    r
}

/// An invite or trade request: the text and two buttons, centered a
/// little above the middle (the group line keeps the top).
pub(super) fn invite(f: &mut Frame<'_>, text: &str) {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.small);
    let bh = f.line_height(Role::Title, s.text) + pad;
    let w = f.px(340.0);
    let r = Rect::new(
        (f.input.screen.0 - w) / 2.0,
        f.input.screen.1 * 0.3,
        w,
        lh + bh + 3.0 * pad,
    );
    f.place("invite", r);
    f.panel(r, true);
    let c = f.theme.colors.clone();
    f.label_fit(
        Role::Text,
        s.small,
        (r.x + pad, r.y + pad),
        r.w - 2.0 * pad,
        c.highlight,
        text,
    );
    let bw = (r.w - 3.0 * pad) / 2.0;
    let y = r.y + 2.0 * pad + lh;
    if f.button(Rect::new(r.x + pad, y, bw, bh), &f.tr("ui.trade.accept")) {
        f.actions.push(UiAction::AnswerInvite(true));
    }
    if f.button(
        Rect::new(r.x + 2.0 * pad + bw, y, bw, bh),
        &f.tr("ui.trade.decline"),
    ) {
        f.actions.push(UiAction::AnswerInvite(false));
    }
}
