//! Everything social done with the mouse, never only by chat command:
//! the group panel (members and Leave), the menu of another player
//! (trade, invite, whisper, lead, remove, report) and the emote list.

use super::*;
use crate::ui::PlayerAct;
use base::EntityId;

/// One member of the player's group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartyMember {
    pub id: EntityId,
    pub name: String,
    pub leader: bool,
    /// The player themself.
    pub me: bool,
    /// Hundredths of their life, when the client sees them.
    pub health: Option<u8>,
}

/// The player's group, leader first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartyModel {
    pub members: Vec<PartyMember>,
    /// The player leads it: may invite, pass the lead and remove.
    pub i_lead: bool,
}

/// The menu of another player, open where it was asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerMenu {
    pub target: EntityId,
    pub name: String,
    pub at: (f32, f32),
    /// They are fallen: Revive comes first.
    pub fallen: bool,
}

/// What the player can do for others: a heal technique known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Support {
    pub heal: bool,
}

/// What the menu offers for `target`, in order.
fn acts(party: Option<&PartyModel>, menu: &PlayerMenu, support: Support) -> Vec<(PlayerAct, &'static str)> {
    let target = menu.target;
    let member = party.is_some_and(|p| p.members.iter().any(|m| m.id == target));
    let lead = party.is_some_and(|p| p.i_lead);
    let mut out = Vec::new();
    if menu.fallen {
        out.push((PlayerAct::Revive, "ui.player.revive"));
    } else if support.heal {
        out.push((PlayerAct::Heal, "ui.player.heal"));
    }
    out.push((PlayerAct::Trade, "ui.player.trade"));
    if !member && (party.is_none() || lead) {
        out.push((PlayerAct::Invite, "ui.player.invite"));
    }
    out.push((PlayerAct::Whisper, "ui.player.whisper"));
    if member && lead {
        out.push((PlayerAct::Lead, "ui.player.lead"));
        out.push((PlayerAct::Kick, "ui.player.kick"));
    }
    out.push((PlayerAct::Report, "ui.player.report"));
    out
}

/// Draws the player menu; returns false once it should close (an option
/// chosen, or a click anywhere else).
pub(super) fn player_menu(
    f: &mut Frame<'_>,
    menu: &PlayerMenu,
    party: Option<&PartyModel>,
    support: Support,
) -> bool {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let bh = f.line_height(Role::Title, s.text) + pad / 2.0;
    let lh = f.line_height(Role::Text, s.small);
    let acts = acts(party, menu, support);
    let w = f.px(180.0);
    let h = lh + pad + acts.len() as f32 * (bh + pad / 4.0) + pad;
    let x = menu.at.0.min(f.input.screen.0 - w - pad).max(pad);
    let y = menu.at.1.min(f.input.screen.1 - h - pad).max(pad);
    let r = Rect::new(x, y, w, h);
    f.panel(r, true);
    let c = f.theme.colors.clone();
    f.label_fit(
        Role::Text,
        s.small,
        (r.x + pad / 2.0, r.y + pad / 2.0),
        r.w - pad,
        c.highlight,
        &menu.name,
    );
    let mut by = r.y + lh + pad;
    for (act, key) in acts {
        let br = Rect::new(r.x + pad / 2.0, by, r.w - pad, bh);
        if f.button(br, &f.tr(key)) {
            f.actions.push(UiAction::Player {
                target: menu.target,
                name: menu.name.clone(),
                act,
            });
            return false;
        }
        by += bh + pad / 4.0;
    }
    let elsewhere = (f.input.clicked || f.input.right_clicked) && !r.contains(f.input.mouse);
    !elsewhere
}

/// The group, compact, at the left under the status strip (the middle
/// stays free for notices): a header with Leave, then one row per member
/// with a thin life bar when they are in sight. A click on another member
/// opens their menu. Returns the menu to open, if any.
pub(super) fn party(f: &mut Frame<'_>, p: &PartyModel) -> Option<PlayerMenu> {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Text, s.small);
    let bar = f.px(3.0);
    let row = lh + bar + pad / 2.0;
    let w = f.px(170.0);
    let h = lh + pad / 2.0 + p.members.len() as f32 * row + pad;
    let r = Rect::new(pad, under_status(f), w, h);
    f.place("party", r);
    f.panel(r, false);
    let c = f.theme.colors.clone();
    let title = f.tr("ui.social.party_title");
    f.label_fit(
        Role::Text,
        s.small,
        (r.x + pad / 2.0, r.y + pad / 4.0),
        r.w / 2.0,
        c.dim_text,
        &title,
    );
    // Leave: a small text button at the header's right.
    let leave = f.tr("ui.social.leave");
    let lw = (f.fonts.measure(Role::Text, f.px(s.small), &leave) + pad).min(r.w / 2.0);
    let lr = Rect::new(r.x + r.w - lw - pad / 4.0, r.y + pad / 4.0, lw, lh);
    let hover = lr.contains(f.input.mouse);
    if hover {
        f.panel(lr, true);
    }
    let color = if hover { c.error } else { c.dim_text };
    f.label_center(Role::Text, s.small, lr, color, &leave);
    if hover && f.input.clicked {
        f.actions.push(UiAction::LeaveParty);
    }
    let mut y = r.y + lh + pad / 2.0;
    let mut open = None;
    for m in &p.members {
        let rr = Rect::new(r.x + pad / 4.0, y, r.w - pad / 2.0, row);
        let hover = !m.me && rr.contains(f.input.mouse);
        if hover {
            f.panel(rr, true);
        }
        let fallen = m.health == Some(0);
        let text = if fallen {
            f.text.t_args("ui.social.fallen", &[Arg::Str(m.name.clone())])
        } else if m.leader {
            f.text.t_args("ui.social.leader", &[Arg::Str(m.name.clone())])
        } else {
            m.name.clone()
        };
        let color = if fallen {
            c.error
        } else if m.me || m.leader {
            c.highlight
        } else {
            c.text
        };
        f.label_fit(
            Role::Text,
            s.small,
            (rr.x + pad / 4.0, rr.y),
            rr.w - pad / 2.0,
            color,
            &text,
        );
        if let Some(hp) = m.health {
            let br = Rect::new(rr.x + pad / 4.0, rr.y + lh, rr.w - pad / 2.0, bar);
            f.painter.bar(f.theme, br, u64::from(hp), 100, c.life);
        }
        if hover && f.input.clicked {
            open = Some(PlayerMenu {
                target: m.id,
                name: m.name.clone(),
                at: f.input.mouse,
                fallen,
            });
        }
        y += row;
    }
    open
}

/// The emote button right of the log tabs and, open, the list above it.
/// Returns whether the list stays open.
pub(super) fn emotes(
    f: &mut Frame<'_>,
    at: (f32, f32),
    h: f32,
    list: &[super::EmoteEntry],
    open: bool,
) -> bool {
    if list.is_empty() {
        return false;
    }
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let label = f.tr("ui.emote.button");
    let bw = f.fonts.measure(Role::Text, f.px(s.small), &label) + pad * 1.5;
    let br = Rect::new(at.0, at.1, bw, h);
    let hover = br.contains(f.input.mouse);
    f.panel(br, hover || open);
    let c = f.theme.colors.clone();
    let color = if hover || open { c.highlight } else { c.dim_text };
    f.label_center(Role::Text, s.small, br, color, &label);
    if hover && f.input.clicked {
        return !open;
    }
    if !open {
        return false;
    }
    let row = f.line_height(Role::Title, s.text) + pad / 2.0;
    let w = f.px(160.0);
    let lh = list.len() as f32 * (row + pad / 4.0) + pad;
    let r = Rect::new(br.x, (br.y - lh - pad / 4.0).max(pad), w, lh);
    f.panel(r, true);
    let mut y = r.y + pad / 2.0;
    for e in list {
        let b = Rect::new(r.x + pad / 2.0, y, r.w - pad, row);
        let clicked = match e.icon {
            // The picture on the left, the name beside it.
            Some(uv) => {
                let hover = b.contains(f.input.mouse);
                f.panel(b, hover);
                let side = row - pad / 2.0;
                f.painter.image(
                    Rect::new(b.x + pad / 4.0, b.y + pad / 4.0, side, side),
                    uv,
                    [1.0; 4],
                );
                let color = if hover { c.highlight } else { c.text };
                let th = f.line_height(Role::Title, s.text);
                let at = (b.x + side + pad * 0.75, b.y + (row - th) / 2.0);
                f.label(Role::Title, s.text, at, color, &e.name);
                hover && f.input.clicked
            }
            None => f.button(b, &e.name),
        };
        if clicked {
            f.actions.push(UiAction::Emote(e.id));
            return false;
        }
        y += row + pad / 4.0;
    }
    !f.input.clicked || r.contains(f.input.mouse)
}
