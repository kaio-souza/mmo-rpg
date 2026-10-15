//! What the HUD shows of a piece with identity (rarity, artisan, power;
//!) and of the group, invites and trade (phase 9).

use crate::graphics::Graphics;
use crate::text::Texts;
use base::{Arg, Fixed, TextRef};
use client_state::WorldState;
use client_ui::{PartyMember, PartyModel, TradeLine, TradeModel, UiText};
use protocol::InstanceInfo;

/// The tooltip lines of one piece: wear, what its rarity adds, who made
/// it (and their mastery) and the power it carries.
pub fn piece_lines(
    state: &WorldState,
    texts: &Texts,
    ui: &UiText,
    p: &InstanceInfo,
    item: base::ItemId,
) -> Vec<String> {
    let mut lines = Vec::new();
    let def = state.item(item);
    let (cur, max) = p.durability;
    if max > 0 {
        lines.push(ui.t_args(
            "ui.item.durability",
            &[Arg::Int(i64::from(cur)), Arg::Int(i64::from(max))],
        ));
    }
    if p.power_permille > 0 {
        let key = if def.is_some_and(|d| d.weapon.is_some()) {
            "ui.item.rarity_damage"
        } else {
            "ui.item.rarity_defense"
        };
        lines.push(ui.t_args(key, &[percent(i64::from(p.power_permille) * 100)]));
    }
    if let Some(c) = &p.crafter {
        if p.mastery > 0 {
            lines.push(ui.t_args(
                "ui.item.crafter_master",
                &[Arg::Str(c.to_string()), percent(i64::from(p.mastery) * 10)],
            ));
        } else {
            lines.push(ui.t_args("ui.item.crafter", &[Arg::Str(c.to_string())]));
        }
    }
    match p.power {
        Some(base::ItemPower::Technique(t)) => {
            let name = state
                .technique_info(t)
                .map_or_else(|| format!("#{}", t.get()), |i| texts.show(&TextRef::new(i.name)));
            lines.push(ui.t_args("ui.item.power_technique", &[Arg::Str(name)]));
        }
        Some(base::ItemPower::Attribute(a, v)) => {
            let label = ui.t(&format!("ui.attr.{}", format!("{a:?}").to_lowercase()));
            lines.push(ui.t_args(
                "ui.item.power_attribute",
                &[Arg::Int(i64::from(v)), Arg::Str(label)],
            ));
        }
        None => {}
    }
    lines
}

/// A percentage from thousandths of a percent (`10`, `0.01`).
fn percent(millis: i64) -> Arg {
    Arg::Fixed(Fixed::from_millis(millis.clamp(0, i64::from(i32::MAX)) as i32))
}

/// The group panel, when the character is in one.
pub fn party(state: &WorldState, texts: &Texts) -> Option<PartyModel> {
    let g = state.party()?;
    let me = state.me().map(|m| m.id);
    Some(PartyModel {
        members: g
            .members
            .iter()
            .map(|m| PartyMember {
                id: *m,
                name: crate::interface::entity_name(state, texts, *m),
                leader: *m == g.leader,
                me: Some(*m) == me,
                health: state.entity(*m).map(|e| e.health),
            })
            .collect(),
        i_lead: me == Some(g.leader),
    })
}

/// The gestures the player can make, named in their language, with their
/// picture when they have one.
pub fn emotes(
    state: &WorldState,
    texts: &Texts,
    gfx: &crate::graphics::Graphics,
) -> Vec<client_ui::EmoteEntry> {
    state
        .emotes()
        .map(|(id, n)| client_ui::EmoteEntry {
            id,
            name: texts.show(&TextRef::new(n)),
            icon: state.emote_icon(id).and_then(|i| gfx.icon_uv(Some(i), "")),
        })
        .collect()
}

/// The newest invite or trade request, written out for its panel.
pub fn invite(state: &WorldState, texts: &Texts, ui: &UiText) -> Option<String> {
    let (from, kind) = state.invite()?;
    // The trade window already asks about its own request.
    if kind == protocol::InviteKind::Trade && state.trade().is_some_and(|t| t.with == from) {
        return None;
    }
    let key = match kind {
        protocol::InviteKind::Party => "ui.social.invite_party",
        protocol::InviteKind::Trade => "ui.social.invite_trade",
    };
    Some(ui.t_args(
        key,
        &[Arg::Str(crate::interface::entity_name(state, texts, from))],
    ))
}

/// The trade window: both offers with every piece's details.
pub fn trade(state: &WorldState, texts: &Texts, ui: &UiText, gfx: &Graphics) -> Option<TradeModel> {
    let t = state.trade()?;
    let line = |item: base::ItemId, slot: u16, (qty, max): (u32, u32), piece: Option<&InstanceInfo>| {
        let def = state.item(item);
        let mut details: Vec<String> = def
            .map(|d| texts.show(&TextRef::new(d.description)))
            .filter(|d| !d.is_empty())
            .into_iter()
            .collect();
        if let Some(p) = piece {
            details.extend(piece_lines(state, texts, ui, p, item));
        }
        TradeLine {
            slot,
            qty,
            max,
            name: def.map_or_else(
                || format!("#{}", item.get()),
                |d| texts.show(&TextRef::new(d.name)),
            ),
            icon: gfx.icon_uv(def.and_then(|d| state.icon_name(d.icon)), "generic_item"),
            rarity: piece.map_or_else(|| def.map_or(base::Rarity::Common, |d| d.rarity), |p| p.rarity),
            details,
        }
    };
    let mine = t
        .mine
        .iter()
        .filter_map(|(slot, qty)| {
            let held = state.inventory()?.slots.get(usize::from(*slot))?.as_ref()?;
            let piece = state.instances().iter().find(|i| i.slot == *slot);
            Some(line(held.item, *slot, (*qty, held.qty.max(1)), piece))
        })
        .collect();
    let theirs = t
        .theirs
        .iter()
        .enumerate()
        .map(|(i, (item, qty))| {
            line(
                *item,
                i as u16,
                (*qty, 0),
                t.theirs_pieces.get(i).and_then(Option::as_ref),
            )
        })
        .collect();
    Some(TradeModel {
        with: crate::interface::entity_name(state, texts, t.with),
        open: t.open,
        asked: !t.open && state.has_invite(t.with, protocol::InviteKind::Trade),
        mine,
        mine_coins: t.mine_coins,
        theirs,
        theirs_coins: t.theirs_coins,
        mine_confirmed: t.mine_confirmed,
        theirs_confirmed: t.theirs_confirmed,
    })
}
