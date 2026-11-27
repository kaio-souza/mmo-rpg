//! `sim` <-> `protocol` and `sim` <-> `persistence`. The simulation knows
//! neither the wire format nor the save format; this module turns its views
//! into frames (each entity encoded once per tick) and its sheets into saves.

use base::TextRef;
use base::{EntityId, PlayerId};
use bytes::Bytes;
use protocol::{EncodedEntity, Entity, EntityKind, EntityName, ErrorCode, ServerMsg};
use sim::{DialogueOut, EntityLabel, EntityState, Refusal, TickOutput};
use std::collections::{BTreeMap, BTreeSet};
use tracing::warn;

pub fn entity(s: &EntityState) -> Entity {
    Entity {
        id: s.id,
        kind: match s.kind {
            sim::EntityKind::Player => EntityKind::Player,
            sim::EntityKind::Npc => EntityKind::Npc,
            sim::EntityKind::Monster => EntityKind::Monster,
            sim::EntityKind::Node => EntityKind::Node,
        },
        pos: s.pos,
        direction: s.direction,
        appearance: s.appearance,
        name: match &s.label {
            EntityLabel::Player(n) => EntityName::Player(n.clone()),
            EntityLabel::Npc(t) => EntityName::Npc(*t),
            EntityLabel::Monster(t) => EntityName::Monster(*t),
            EntityLabel::Node(t) => EntityName::Node(*t),
        },
        health: s.health,
        equipment: s.equipment,
        light: s.light,
        level: s.level,
        look: s.look,
    }
}

/// `Appearances`: the creation lists, sent with `Session`.
pub fn appearances(catalog: &defs::ValidatedCatalog) -> Result<Bytes, protocol::ProtocolError> {
    let a = catalog.appearance();
    let list = |l: &[defs::LookOption]| {
        l.iter()
            .map(|o| protocol::LookOptionInfo {
                name: o.name,
                color: o.color,
                sprite: o.sprite.clone(),
                clothes: o.clothes.clone(),
            })
            .collect()
    };
    let msg = ServerMsg::Appearances(protocol::AppearanceInfo {
        lists: [
            list(a.bodies()),
            list(a.skins()),
            list(a.hair_styles()),
            list(a.hair_colors()),
        ],
    });
    let mut buf = Vec::new();
    protocol::encode(&msg, &mut buf)?;
    Ok(Bytes::from(buf))
}

/// One frame per player whose view changed. Players in `fresh` (entered
/// this tick) get a `Snapshot`, everyone else a `Diff`.
pub fn frames(out: &TickOutput, fresh: &BTreeSet<PlayerId>) -> Vec<(PlayerId, Bytes)> {
    let mut encoded: BTreeMap<EntityId, EncodedEntity> = BTreeMap::new();
    for s in &out.entities {
        match protocol::encode_entity(&entity(s)) {
            Ok(e) => {
                encoded.insert(s.id, e);
            }
            Err(e) => warn!(error = %e, "encode entity"),
        }
    }
    let tick = out.tick.0 as u32;
    let mut frames = Vec::with_capacity(out.views.len());
    let mut pieces: Vec<EncodedEntity> = Vec::new();
    let mut updated: Vec<EncodedEntity> = Vec::new();
    let mut buf = Vec::new();
    for (player, view) in &out.views {
        pieces.clear();
        pieces.extend(view.entered.iter().filter_map(|id| encoded.get(id).cloned()));
        buf.clear();
        let res = if fresh.contains(player) {
            protocol::encode_snapshot(&pieces, &mut buf)
        } else {
            updated.clear();
            updated.extend(view.updated.iter().filter_map(|id| encoded.get(id).cloned()));
            protocol::encode_diff(tick, &pieces, &view.left, &view.moved, &updated, &mut buf)
        };
        match res {
            Ok(()) => frames.push((*player, Bytes::copy_from_slice(&buf))),
            Err(e) => warn!(error = %e, "encode view"),
        }
    }
    frames
}

// Content may not hold more options than the wire allows.
const _: () = assert!(defs::MAX_OPTIONS == protocol::MAX_DIALOGUE_OPTIONS);

/// Dialogue results of the tick, one frame each, in command order.
pub fn dialogue_frames(out: &TickOutput) -> Vec<(PlayerId, Bytes)> {
    let mut frames = Vec::with_capacity(out.dialogue.len());
    for d in &out.dialogue {
        let (player, msg) = match d {
            DialogueOut::Show {
                player,
                npc,
                text,
                options,
            } => (
                *player,
                ServerMsg::Dialogue {
                    npc: *npc,
                    text: TextRef::new(*text),
                    options: options.iter().map(|t| TextRef::new(*t)).collect(),
                },
            ),
            DialogueOut::End { player, npc } => (*player, ServerMsg::DialogueEnd { npc: *npc }),
            DialogueOut::Refused { player, reason } => (*player, refusal(*reason)),
        };
        let mut buf = Vec::new();
        match protocol::encode(&msg, &mut buf) {
            Ok(()) => frames.push((player, Bytes::from(buf))),
            Err(e) => warn!(error = %e, "encode dialogue"),
        }
    }
    frames
}

fn refusal(reason: Refusal) -> ServerMsg {
    ServerMsg::Error {
        code: match reason {
            Refusal::TooFar => ErrorCode::TooFar,
            Refusal::NoDialogue => ErrorCode::NoDialogue,
            Refusal::InventoryFull => ErrorCode::InventoryFull,
            Refusal::RequirementsNotMet => ErrorCode::RequirementsNotMet,
            Refusal::InvalidSlot => ErrorCode::InvalidSlot,
            Refusal::InvalidTarget => ErrorCode::InvalidTarget,
            Refusal::NotReady => ErrorCode::NotReady,
            Refusal::Fallen => ErrorCode::Fallen,
            Refusal::NoShop => ErrorCode::NoShop,
            Refusal::NotEnoughCoins => ErrorCode::NotEnoughCoins,
            Refusal::OutOfStock => ErrorCode::OutOfStock,
            Refusal::NotTraded => ErrorCode::NotTraded,
            Refusal::CannotCraft => ErrorCode::CannotCraft,
            Refusal::NothingToGather => ErrorCode::NothingToGather,
            Refusal::NoParty => ErrorCode::NoParty,
            Refusal::PartyFull => ErrorCode::PartyFull,
            Refusal::NoTrade => ErrorCode::NoTrade,
            Refusal::TradeChanged => ErrorCode::TradeChanged,
            Refusal::NothingToUse => ErrorCode::NothingToUse,
        },
        args: Vec::new(),
    }
}

fn push(frames: &mut Vec<(PlayerId, Bytes)>, player: PlayerId, msg: &ServerMsg) {
    let mut buf = Vec::new();
    match protocol::encode(msg, &mut buf) {
        Ok(()) => frames.push((player, Bytes::from(buf))),
        Err(e) => warn!(error = %e, "encode player frame"),
    }
}

/// A piece with identity as the client sees it.
fn instance_info(i: &sim::InstanceView) -> protocol::InstanceInfo {
    protocol::InstanceInfo {
        slot: i.slot,
        id: i.id,
        hidden: i.hidden.clone(),
        durability: i.durability,
        crafter: i.crafter.clone(),
        rarity: i.rarity,
        power_permille: i.power_permille,
        mastery: i.mastery,
        power: i.power,
    }
}

/// Refused `MoveItem`s, backpacks and quest logs of the tick.
pub fn player_frames(out: &TickOutput) -> Vec<(PlayerId, Bytes)> {
    let mut frames = Vec::new();
    for (player, reason) in &out.refused {
        push(&mut frames, *player, &refusal(*reason));
    }
    for (player, inv) in &out.inventories {
        let msg = ServerMsg::Inventory {
            coins: inv.coins,
            slots: inv
                .slots
                .iter()
                .map(|s| s.map(|(item, qty)| protocol::ItemSlot { item, qty }))
                .collect(),
            instances: inv.instances.iter().map(instance_info).collect(),
            equipped: inv.equipped.iter().map(instance_info).collect(),
        };
        push(&mut frames, *player, &msg);
    }
    for (player, log) in &out.quest_logs {
        let msg = ServerMsg::QuestLog {
            quests: log
                .iter()
                .map(|q| protocol::QuestEntry {
                    name: q.name,
                    description: q.description,
                    state: if q.completed {
                        protocol::QuestState::Completed
                    } else {
                        protocol::QuestState::Active
                    },
                    goal: q.goal,
                    objectives: q
                        .objectives
                        .iter()
                        .map(|o| match o {
                            sim::ObjectiveView::Item { item, have, need } => protocol::ObjectiveView::Item {
                                item: *item,
                                have: *have,
                                need: *need,
                            },
                            sim::ObjectiveView::Talk { npc, done } => protocol::ObjectiveView::Talk {
                                npc: *npc,
                                done: *done,
                            },
                            sim::ObjectiveView::Kill { monster, have, need } => {
                                protocol::ObjectiveView::Kill {
                                    monster: *monster,
                                    have: *have,
                                    need: *need,
                                }
                            }
                            sim::ObjectiveView::Craft { item, have, need } => {
                                protocol::ObjectiveView::Craft {
                                    item: *item,
                                    have: *have,
                                    need: *need,
                                }
                            }
                            sim::ObjectiveView::Reach { pos, done } => protocol::ObjectiveView::Reach {
                                pos: *pos,
                                done: *done,
                            },
                        })
                        .collect(),
                })
                .collect(),
        };
        push(&mut frames, *player, &msg);
    }
    for (player, st) in &out.stats {
        let a = st.attributes;
        let msg = ServerMsg::Stats(Box::new(protocol::StatsInfo {
            level: st.level,
            xp: st.xp,
            xp_next: st.xp_next,
            hp: st.hp,
            max_hp: st.max_hp,
            vigor: st.vigor,
            max_vigor: st.max_vigor,
            mp: st.mp,
            max_mp: st.max_mp,
            attributes: [a.str, a.vit, a.agi, a.dex, a.int, a.spi],
            free_points: st.free_points,
            class: st.class,
            classes_available: st.classes_available.clone(),
            proficiencies: st.proficiencies.clone(),
            techniques: st.techniques.clone(),
            titles: st.titles.clone(),
            equipment: st.equipment.iter().map(|(s, i)| (equip_slot(*s), *i)).collect(),
            defense: st.defense,
            fallen: st.fallen,
            cooldowns: st.cooldowns.clone(),
            global_cooldown: st.global_cooldown,
            item_cooldowns: st.item_cooldowns.clone(),
            recipes: st.recipes.clone(),
            base_attributes: {
                let b = st.base_attributes;
                [b.str, b.vit, b.agi, b.dex, b.int, b.spi]
            },
            appraises: st.appraises,
            specials: st.specials.clone(),
            // Every production class mends its own pieces.
            self_repairs: st.appraises,
        }));
        push(&mut frames, *player, &msg);
    }
    for (player, text, window) in out
        .system
        .iter()
        .map(|(p, t)| (p, t, true))
        .chain(out.notes.iter().map(|(p, t)| (p, t, false)))
    {
        let msg = ServerMsg::System {
            text: text.clone(),
            window,
        };
        push(&mut frames, *player, &msg);
    }
    for (player, npcs) in &out.shop_npcs {
        let msg = ServerMsg::Shops { npcs: npcs.clone() };
        push(&mut frames, *player, &msg);
    }
    for (player, shop) in &out.shops {
        let msg = match shop {
            None => ServerMsg::ShopEnd,
            Some(s) => ServerMsg::Shop(Box::new(protocol::ShopView {
                npc: s.npc,
                stock: s
                    .stock
                    .iter()
                    .map(|l| protocol::ShopLine {
                        item: l.item,
                        qty: l.qty,
                        price: l.price,
                    })
                    .collect(),
                pays_permille: s.pays_permille,
                repair_permille: s.repair_permille,
                price_permille: s.price_permille,
                appraise_permille: s.appraise_permille,
                purse: s.purse,
            })),
        };
        push(&mut frames, *player, &msg);
    }
    for inv in &out.invites {
        let msg = ServerMsg::Invite {
            from: inv.from,
            kind: match inv.kind {
                sim::InviteKind::Party => protocol::InviteKind::Party,
                sim::InviteKind::Trade => protocol::InviteKind::Trade,
            },
        };
        push(&mut frames, inv.to, &msg);
    }
    for (viewer, entity, emote) in &out.emotes {
        let msg = ServerMsg::Emote {
            entity: *entity,
            emote: *emote,
        };
        push(&mut frames, *viewer, &msg);
    }
    for (player, party) in &out.parties {
        let msg = match party {
            None => ServerMsg::PartyEnd,
            Some(g) => ServerMsg::Party(Box::new(protocol::PartyView {
                leader: g.leader,
                members: g.members.clone(),
            })),
        };
        push(&mut frames, *player, &msg);
    }
    for (player, trade) in &out.trades {
        let msg = match trade {
            None => ServerMsg::TradeEnd,
            Some(t) => ServerMsg::Trade(Box::new(protocol::TradeView {
                with: t.with,
                open: t.open,
                mine: t.mine.clone(),
                mine_coins: t.mine_coins,
                theirs: t.theirs.clone(),
                theirs_pieces: t
                    .theirs_pieces
                    .iter()
                    .map(|p| p.as_ref().map(instance_info))
                    .collect(),
                theirs_coins: t.theirs_coins,
                mine_confirmed: t.mine_confirmed,
                theirs_confirmed: t.theirs_confirmed,
            })),
        };
        push(&mut frames, *player, &msg);
    }
    for (player, marks) in &out.markers {
        let msg = ServerMsg::NpcMarkers {
            markers: marks
                .iter()
                .map(|(id, m)| {
                    (
                        *id,
                        match m {
                            sim::QuestMarker::Available => protocol::QuestMarker::Available,
                            sim::QuestMarker::Talk => protocol::QuestMarker::Talk,
                            sim::QuestMarker::Ready => protocol::QuestMarker::Ready,
                        },
                    )
                })
                .collect(),
        };
        push(&mut frames, *player, &msg);
    }
    frames
}

pub fn equip_slot(s: sim::EquipSlot) -> protocol::EquipSlot {
    match s {
        sim::EquipSlot::Weapon => protocol::EquipSlot::Weapon,
        sim::EquipSlot::Armor => protocol::EquipSlot::Armor,
        sim::EquipSlot::Offhand => protocol::EquipSlot::Offhand,
    }
}

/// Blows go to every player of the zone who knows the attacker or the target.
pub fn combat_frames(
    out: &TickOutput,
    players: &[PlayerId],
    known: &dyn Fn(PlayerId, EntityId) -> bool,
) -> Vec<(PlayerId, Bytes)> {
    let mut frames = Vec::new();
    if out.combat.is_empty() && out.supports.is_empty() {
        return frames;
    }
    for &player in players {
        let used: Vec<protocol::TechniqueUse> = out
            .supports
            .iter()
            .filter(|&&(user, target, _)| known(player, user) || known(player, target))
            .map(|&(user, target, technique)| protocol::TechniqueUse {
                user,
                target,
                technique,
            })
            .collect();
        if !used.is_empty() {
            push(&mut frames, player, &ServerMsg::TechniqueUsed { events: used });
        }
        let events: Vec<protocol::CombatEvent> = out
            .combat
            .iter()
            .filter(|c| known(player, c.target) || known(player, c.attacker))
            .map(|c| protocol::CombatEvent {
                attacker: c.attacker,
                target: c.target,
                damage: c.damage,
                crit: c.crit,
                killed: c.killed,
                technique: c.technique,
            })
            .collect();
        if !events.is_empty() {
            push(&mut frames, player, &ServerMsg::Combat { events });
        }
    }
    frames
}

/// A phase 4 client message as a zone command.
pub fn fight_command(player: PlayerId, msg: protocol::ClientMsg) -> Option<sim::Command> {
    use protocol::ClientMsg as M;
    use sim::Command as C;
    Some(match msg {
        M::Attack { target } => C::Attack { player, target },
        M::UseTechnique { technique, target } => C::UseTechnique {
            player,
            technique,
            target,
        },
        M::AllocatePoints { points } => {
            let [str, vit, agi, dex, int, spi] = points;
            C::AllocatePoints {
                player,
                points: defs::Attributes {
                    str,
                    vit,
                    agi,
                    dex,
                    int,
                    spi,
                },
            }
        }
        M::ChooseClass { class } => C::ChooseClass { player, class },
        M::Equip { slot } => C::Equip { player, slot },
        M::Unequip { slot } => C::Unequip {
            player,
            slot: match slot {
                protocol::EquipSlot::Weapon => sim::EquipSlot::Weapon,
                protocol::EquipSlot::Armor => sim::EquipSlot::Armor,
                protocol::EquipSlot::Offhand => sim::EquipSlot::Offhand,
            },
        },
        M::Appraise { slot } => C::Appraise { player, slot },
        M::GiveUp => C::GiveUp { player },
        M::UseItem { slot } => C::UseItem { player, slot },
        M::UseItemOn { slot, target } => C::UseItemOn { player, slot, target },
        M::CloseDialogue => C::CloseDialogue { player },
        M::OpenShop { target } => C::OpenShop { player, target },
        M::CloseShop => C::CloseShop { player },
        M::Buy { index, qty } => C::Buy { player, index, qty },
        M::Sell { slot, qty } => C::Sell { player, slot, qty },
        M::Repair { slot } => C::Repair { player, slot },
        M::ShopAppraise { slot } => C::ShopAppraise { player, slot },
        M::SelfRepair { slot } => C::SelfRepair { player, slot },
        M::Craft { recipe } => C::Craft { player, recipe },
        M::Gather { target } => C::Gather { player, target },
        M::UseTile { pos } => C::UseTile { player, pos },
        M::PartyInvite { target } => C::PartyInvite { player, target },
        M::AnswerInvite { accept } => C::AnswerInvite { player, accept },
        M::LeaveParty => C::LeaveParty { player },
        M::PartyKick { target } => C::PartyKick { player, target },
        M::PartyLead { target } => C::PartyLead { player, target },
        M::TradeRequest { target } => C::TradeRequest { player, target },
        M::TradeOffer { items, coins } => C::TradeOffer { player, items, coins },
        M::TradeConfirm => C::TradeConfirm { player },
        M::TradeCancel => C::TradeCancel { player },
        M::Emote { emote } => C::Emote { player, emote },
        _ => return None,
    })
}

/// `Definitions` of the content, encoded once.
pub fn definitions(catalog: &defs::ValidatedCatalog) -> Result<Bytes, protocol::ProtocolError> {
    let unarmed = catalog.proficiency_by_name(defs::UNARMED).map(|(id, _)| id);
    let msg = ServerMsg::Definitions {
        content_hash: catalog.content_hash(),
        items: catalog
            .items()
            .iter()
            .map(|d| protocol::ItemInfo {
                name: d.name,
                description: d.description,
                kind: d.kind,
                rarity: d.rarity,
                max_stack: d.max_stack,
                weight: d.weight,
                use_effect: d.use_effect.map(|u| protocol::UseInfo {
                    hp: u.hp,
                    vigor: u.vigor,
                    cooldown_ticks: u.cooldown_ticks,
                    revive_permille: u.revive_permille,
                    mp: u.mp,
                }),
                value: d.value,
                durability: d.durability,
                icon: catalog.icon_index(&d.icon),
                weapon: match &d.equip {
                    Some(defs::Equip::Weapon { proficiency, .. }) => Some(*proficiency),
                    _ => None,
                },
                light: d.light.map(|l| protocol::LightInfo {
                    radius: l.radius,
                    color: [l.color.0, l.color.1, l.color.2],
                }),
                burn: d.burn,
                equip: d.equip.as_ref().map(|e| match e {
                    defs::Equip::Weapon {
                        damage,
                        attack_ticks,
                        attribute,
                        range,
                        ..
                    } => protocol::EquipInfo {
                        damage: *damage,
                        attack_ticks: *attack_ticks,
                        range: *range,
                        defense: 0,
                        attribute: base::Attribute::ALL
                            .iter()
                            .position(|a| a == attribute)
                            .and_then(|i| u8::try_from(i).ok())
                            .unwrap_or(0),
                    },
                    defs::Equip::Armor { defense, .. } | defs::Equip::Offhand { defense, .. } => {
                        protocol::EquipInfo {
                            damage: (0, 0),
                            attack_ticks: 0,
                            range: 0,
                            defense: *defense,
                            attribute: 0,
                        }
                    }
                }),
            })
            .collect(),
        classes: catalog
            .classes()
            .map(|(_, c)| protocol::NamedInfo {
                name: c.name,
                description: Some(c.description),
                icon: catalog.icon_index(&c.icon),
            })
            .collect(),
        techniques: catalog
            .techniques()
            .map(|(_, t)| protocol::TechniqueInfo {
                name: t.name,
                description: t.description,
                vigor: t.vigor,
                mp: t.mp,
                cooldown_ticks: t.cooldown_ticks,
                range: t.range,
                damage_permille: t.damage_permille,
                growth_permille: t.growth_permille,
                icon: catalog.icon_index(&t.icon),
                proficiency: Some(t.proficiency).filter(|p| Some(*p) != unarmed),
                any_weapon: catalog
                    .proficiency(t.proficiency)
                    .is_some_and(|p| p.kind == defs::ProficiencyKind::Magic),
                heal_permille: t.heal_permille,
                revive_permille: t.revive_permille,
                buff_permille: t.buff.map_or(0, |b| b.permille),
                buff_ticks: t.buff.map_or(0, |b| b.duration_ticks),
                id: t.id.clone(),
            })
            .collect(),
        proficiencies: (0..)
            .map_while(|i| catalog.proficiency(base::ProficiencyId::new(i)))
            .map(|p| protocol::NamedInfo {
                name: p.name,
                description: None,
                icon: 0,
            })
            .collect(),
        titles: catalog
            .titles()
            .map(|(_, t)| protocol::NamedInfo {
                name: t.name,
                description: Some(t.description),
                icon: catalog.icon_index(&t.icon),
            })
            .collect(),
        sprites: catalog.sprites().to_vec(),
        icons: catalog.icons().to_vec(),
        recipes: catalog
            .recipes()
            .map(|(_, r)| protocol::RecipeInfo {
                name: r.name,
                description: r.description,
                proficiency: r.proficiency,
                min_points: r.min_points,
                materials: r.materials.iter().map(|m| (m.item, m.qty)).collect(),
                result: r.result,
                qty: r.qty,
            })
            .collect(),
        emotes: catalog.emotes().map(|(_, e)| e.name).collect(),
        emote_icons: catalog
            .emotes()
            .map(|(_, e)| catalog.icon_index(&e.icon))
            .collect(),
        zones: catalog
            .zones()
            .map(|(_, z)| protocol::ZoneInfo {
                name: z.name.clone(),
                min: z.origin,
                max: base::TilePos::new(z.origin.x + z.width - 1, z.origin.y + z.height - 1),
            })
            .collect(),
        title_bonuses: catalog
            .titles()
            .map(|(_, t)| {
                t.bonuses
                    .iter()
                    .filter_map(|b| title_bonus_info(catalog, b))
                    .collect()
            })
            .collect(),
    };
    let mut buf = Vec::new();
    protocol::encode(&msg, &mut buf)?;
    Ok(Bytes::from(buf))
}

/// One title bonus as the tooltip shows it.
fn title_bonus_info(
    catalog: &defs::ValidatedCatalog,
    b: &defs::TitleBonus,
) -> Option<protocol::TitleBonusInfo> {
    use protocol::TitleBonusInfo as I;
    let percent = |kind: defs::PercentBonus, v: u16, until: Option<u8>| match kind {
        defs::PercentBonus::Damage => I::DamagePercent(v, until),
        defs::PercentBonus::Xp => I::XpPercent(v, until),
        defs::PercentBonus::MaxMp => I::MaxMpPercent(v, until),
    };
    Some(match b {
        defs::TitleBonus::Attribute(a, v) => I::Attribute(*a, *v),
        defs::TitleBonus::MaxHp(v) => I::MaxHp(*v),
        defs::TitleBonus::MaxVigor(v) => I::MaxVigor(*v),
        defs::TitleBonus::Percent(k, v) => percent(*k, *v, None),
        defs::TitleBonus::PercentUntilLevel(k, v, l) => percent(*k, *v, Some(*l)),
        defs::TitleBonus::Special(id, v) => I::Special(
            catalog.special_attributes().iter().find(|s| s.id == *id)?.name,
            *v,
        ),
    })
}

/// Sheet for the simulation from what was loaded.
pub fn sheet_to_sim(name: base::CharacterName, saved: &persistence::SavedSheet) -> sim::CharacterSheet {
    sim::CharacterSheet {
        name,
        look: saved.look.clone(),
        coins: saved.coins,
        backpack: saved
            .backpack
            .iter()
            .map(|s| {
                s.as_ref().map(|s| sim::SavedStack {
                    item: s.item.clone(),
                    qty: s.qty,
                })
            })
            .collect(),
        quests: saved
            .quests
            .iter()
            .map(|q| sim::SavedQuest {
                quest: q.quest.clone(),
                completed: q.completed,
                talked: q.talked,
                completed_at: q.completed_at,
                progress: q.progress.clone(),
            })
            .collect(),
        affinity: saved.affinity.clone(),
        progress: {
            let p = &saved.progress;
            let [str, vit, agi, dex, int, spi] = p.attributes;
            sim::SavedProgress {
                level: p.level,
                xp: p.xp,
                max_level: p.max_level,
                attributes: defs::Attributes {
                    str,
                    vit,
                    agi,
                    dex,
                    int,
                    spi,
                },
                free_points: p.free_points,
                class: p.class.clone(),
                proficiencies: p.proficiencies.clone(),
                techniques: p.techniques.clone(),
                titles: p.titles.clone(),
                counters: p.counters.clone(),
            }
        },
        items: saved.items.iter().map(item_to_sim).collect(),
        recipes: saved.recipes.clone(),
        hp: saved.hp,
        locked_until: saved.locked_until,
        shop_sales: saved.shop_sales.clone(),
    }
}

/// Rarity by position, the order saved on disk (append only).
const RARITIES: [base::Rarity; 6] = [
    base::Rarity::Common,
    base::Rarity::Rare,
    base::Rarity::Epic,
    base::Rarity::Unique,
    base::Rarity::Legendary,
    base::Rarity::Mythic,
];

/// What is saved of a simulation sheet (the name lives in its own column).
pub fn sheet_to_saved(sheet: &sim::CharacterSheet) -> persistence::SavedSheet {
    persistence::SavedSheet {
        look: sheet.look.clone(),
        coins: sheet.coins,
        backpack: sheet
            .backpack
            .iter()
            .map(|s| {
                s.as_ref().map(|s| persistence::SavedStack {
                    item: s.item.clone(),
                    qty: s.qty,
                })
            })
            .collect(),
        quests: sheet
            .quests
            .iter()
            .map(|q| persistence::SavedQuest {
                quest: q.quest.clone(),
                completed: q.completed,
                talked: q.talked,
                completed_at: q.completed_at,
                progress: q.progress.clone(),
            })
            .collect(),
        affinity: sheet.affinity.clone(),
        progress: {
            let p = &sheet.progress;
            let a = p.attributes;
            persistence::SavedProgress {
                level: p.level,
                xp: p.xp,
                max_level: p.max_level,
                attributes: [a.str, a.vit, a.agi, a.dex, a.int, a.spi],
                free_points: p.free_points,
                class: p.class.clone(),
                proficiencies: p.proficiencies.clone(),
                techniques: p.techniques.clone(),
                titles: p.titles.clone(),
                counters: p.counters.clone(),
            }
        },
        items: sheet.items.iter().map(item_to_saved).collect(),
        recipes: sheet.recipes.clone(),
        hp: sheet.hp,
        locked_until: sheet.locked_until,
        shop_sales: sheet.shop_sales.clone(),
    }
}

/// One piece as the simulation sees it.
fn item_to_sim(i: &persistence::SavedItem) -> sim::SavedItem {
    sim::SavedItem {
        id: i.id,
        item: i.item.clone(),
        place: match i.equipped {
            Some(0) => sim::ItemPlace::Equipped(sim::EquipSlot::Weapon),
            Some(2) => sim::ItemPlace::Equipped(sim::EquipSlot::Offhand),
            Some(_) => sim::ItemPlace::Equipped(sim::EquipSlot::Armor),
            None => sim::ItemPlace::Backpack(i.slot),
        },
        hidden: i
            .hidden
            .iter()
            .filter_map(|(a, v)| Some((*base::Attribute::ALL.get(usize::from(*a))?, *v)))
            .collect(),
        appraised: i.appraised,
        durability: i.durability,
        crafter: i.crafter.clone(),
        rarity: i.rarity.and_then(|r| RARITIES.get(usize::from(r)).copied()),
        mastery: i.mastery,
        power: i.power.as_ref().and_then(|p| match p {
            persistence::SavedPower::Technique(t) => Some(sim::SavedPower::Technique(t.clone())),
            persistence::SavedPower::Attribute(a, v) => base::Attribute::ALL
                .get(usize::from(*a))
                .map(|a| sim::SavedPower::Attribute(*a, *v)),
        }),
    }
}

/// One piece as saved: id, crafter, rarity, wear and bonuses, all of it.
fn item_to_saved(i: &sim::SavedItem) -> persistence::SavedItem {
    persistence::SavedItem {
        id: i.id,
        item: i.item.clone(),
        equipped: match i.place {
            sim::ItemPlace::Equipped(sim::EquipSlot::Weapon) => Some(0),
            sim::ItemPlace::Equipped(sim::EquipSlot::Armor) => Some(1),
            sim::ItemPlace::Equipped(sim::EquipSlot::Offhand) => Some(2),
            sim::ItemPlace::Backpack(_) => None,
        },
        slot: match i.place {
            sim::ItemPlace::Backpack(s) => s,
            sim::ItemPlace::Equipped(_) => 0,
        },
        hidden: i
            .hidden
            .iter()
            .filter_map(|(a, v)| Some((base::Attribute::ALL.iter().position(|x| x == a)? as u8, *v)))
            .collect(),
        appraised: i.appraised,
        durability: i.durability,
        crafter: i.crafter.clone(),
        rarity: i
            .rarity
            .and_then(|r| RARITIES.iter().position(|x| *x == r))
            .and_then(|i| u8::try_from(i).ok()),
        mastery: i.mastery,
        power: i.power.as_ref().and_then(|p| match p {
            sim::SavedPower::Technique(t) => Some(persistence::SavedPower::Technique(t.clone())),
            sim::SavedPower::Attribute(a, v) => base::Attribute::ALL
                .iter()
                .position(|x| x == a)
                .and_then(|i| u8::try_from(i).ok())
                .map(|i| persistence::SavedPower::Attribute(i, *v)),
        }),
    }
}

/// A parcel of custody on its way to the database (rule 4).
pub fn parcel_to_saved(p: &sim::EscrowParcel) -> persistence::EscrowParcel {
    persistence::EscrowParcel {
        coins: p.coins,
        stacks: p
            .stacks
            .iter()
            .map(|s| persistence::SavedStack {
                item: s.item.clone(),
                qty: s.qty,
            })
            .collect(),
        items: p.items.iter().map(item_to_saved).collect(),
    }
}

pub fn parcel_to_sim(p: &persistence::EscrowParcel) -> sim::EscrowParcel {
    sim::EscrowParcel {
        coins: p.coins,
        stacks: p
            .stacks
            .iter()
            .map(|s| sim::SavedStack {
                item: s.item.clone(),
                qty: s.qty,
            })
            .collect(),
        items: p.items.iter().map(item_to_sim).collect(),
    }
}

#[cfg(test)]
mod tests {
    /// The client shows `error.<code>`: every code needs its text.
    #[test]
    fn every_error_code_has_a_text() {
        let content = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
        // Only the source key matters here; `validate content/` is what
        // guards the ten locales.
        let mut raw = defs::load(&content).unwrap();
        let _ = raw.fill_missing_text();
        let catalog = raw.validate().unwrap();
        for code in protocol::ErrorCode::ALL {
            assert!(
                catalog.keys().id(code.text_key()).is_some(),
                "{} missing",
                code.text_key()
            );
        }
    }
}
