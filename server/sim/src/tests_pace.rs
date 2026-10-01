//! Pace of the alpha map: a simulated hunter per level band, with the real
//! content and gear of its level. It walks out of the zone's safe camp,
//! pulls the stray prey first, uses its sword techniques and a potion below
//! a third of its HP, and rests to 90% between pulls. It never retreats, so
//! its falls are a floor, not a player's: compare bands with it, and keep
//! the hand play for the absolute pace. The table lives in
//! Slow in debug, so it is ignored:
//! `cargo test --release -p sim pace -- --ignored --nocapture`.

use crate::fight::{Fighter, MonsterComp};
use crate::zone::{Body, PlayerComp};
use crate::*;
use base::{CharacterName, EntityId, PlayerId, TilePos};
use std::sync::Arc;

const TICKS_PER_HOUR: u64 = 20 * 3600;
/// Sword techniques the hunter uses on cooldown.
const TECHNIQUES: &[&str] = &["double_cut", "pierce"];
/// Simulated time per band, in tenths of an hour.
const HOURS_TENTHS: u64 = 10;
/// Stacks of 15 potions the hunter carries.
const POTION_STACKS: u16 = 6;

struct Band {
    zone: &'static str,
    prey: &'static [&'static str],
    level: u8,
    weapon: &'static str,
    armor: &'static str,
}

const BANDS: &[Band] = &[
    Band {
        zone: "dev_field",
        prey: &["field_rat", "wolf"],
        level: 2,
        weapon: "rusty_sword",
        armor: "cloth_tunic",
    },
    Band {
        zone: "briar_hills",
        prey: &["briar_spider", "hill_goblin"],
        level: 4,
        weapon: "iron_sword",
        armor: "leather_armor",
    },
    Band {
        zone: "briar_hills",
        prey: &["hill_goblin", "goblin_shaman", "goblin_brute"],
        level: 6,
        weapon: "iron_sword",
        armor: "studded_leather",
    },
    Band {
        zone: "whisperwood",
        prey: &["dire_wolf", "bandit_scout", "bandit"],
        level: 8,
        weapon: "iron_sword",
        armor: "iron_chainmail",
    },
    Band {
        zone: "fenmoor",
        prey: &["bog_frog", "marsh_slime", "fen_wisp"],
        level: 9,
        weapon: "iron_sword",
        armor: "iron_chainmail",
    },
    Band {
        zone: "ironridge",
        prey: &["cave_bat", "kobold_miner"],
        level: 11,
        weapon: "iron_sword",
        armor: "iron_plate_armor",
    },
    Band {
        zone: "ironridge",
        prey: &["cave_spider", "kobold_foreman", "kobold_miner"],
        level: 13,
        weapon: "iron_sword",
        armor: "iron_plate_armor",
    },
];

fn potion(level: u8) -> &'static str {
    match level {
        0..=6 => "minor_health_potion",
        7..=10 => "health_potion",
        _ => "greater_health_potion",
    }
}

fn hunter(b: &Band) -> CharacterSheet {
    let mut s = CharacterSheet::new(CharacterName::try_from("hunter").unwrap());
    let points = 3 * u16::from(b.level - 1);
    s.progress.level = b.level;
    s.progress.max_level = b.level;
    // A melee build: half to STR, the rest to VIT and DEX.
    s.progress.attributes = defs::Attributes {
        str: 5 + points / 2,
        vit: 5 + points * 3 / 10,
        dex: 5 + points - points / 2 - points * 3 / 10,
        ..defs::Attributes::all(5)
    };
    s.progress.proficiencies = vec![("sword".into(), 2 * b.level, 0)];
    s.progress.techniques = TECHNIQUES.iter().map(|t| ((*t).into(), 1, 0)).collect();
    // Potions of its tier in slot 0, as a player carries them.
    s.backpack = (0..POTION_STACKS)
        .map(|_| {
            Some(SavedStack {
                item: potion(b.level).into(),
                qty: 15,
            })
        })
        .collect();
    for (id, (item, slot)) in [(b.weapon, EquipSlot::Weapon), (b.armor, EquipSlot::Armor)]
        .into_iter()
        .enumerate()
    {
        s.items.push(SavedItem {
            id: id as u64 + 1,
            item: item.into(),
            place: ItemPlace::Equipped(slot),
            hidden: Vec::new(),
            appraised: true,
            durability: (0, 0),
            crafter: None,
            rarity: None,
            mastery: 0,
            power: None,
        });
    }
    s
}

struct Result {
    xp_per_hour: u64,
    potions: u32,
    kills: u32,
    falls: u32,
}

fn hunt(content: &Arc<Content>, b: &Band, hours_tenths: u64) -> Result {
    let cat = &content.catalog;
    let (zone, _) = cat.zone_by_name(b.zone).unwrap();
    let prey: Vec<base::MonsterId> = b.prey.iter().map(|n| cat.monster_by_name(n).unwrap().0).collect();
    let techniques: Vec<base::TechniqueId> = TECHNIQUES
        .iter()
        .map(|t| cat.technique_by_name(t).unwrap().0)
        .collect();
    let mut z = Zone::new(content.clone(), zone, 7).unwrap();
    let mut out = TickOutput::default();
    let me = PlayerId::new(1);
    let t0 = 1_700_000_000;
    // Enters on the zone's spawn, the safe camp where a fallen newbie comes
    // back too, and walks from there like a player.
    let mut pending = vec![
        Command::Enter {
            player: me,
            sheet: Box::new(hunter(b)),
            pos: None,
        },
        Command::ItemIds {
            start: 1000,
            end: 100_000,
        },
    ];
    let (mut kills, mut falls, mut potions) = (0u32, 0u32, 0u32);
    let mut last: Option<CharacterSheet> = None;
    let mut xp = 0u64;
    let f = cat.formulas();
    let gained = |s: &Option<CharacterSheet>| {
        s.as_ref().map_or(0, |s| {
            let levels: u64 = (b.level..s.progress.level).map(|l| f.level_xp(l)).sum();
            s.progress.xp + levels
        })
    };
    let mut target: Option<EntityId> = None;
    let ticks = TICKS_PER_HOUR * hours_tenths / 10;
    for t in 0..ticks {
        z.tick(
            &TickInput {
                commands: &pending,
                now_unix: t0 + t / 20,
                load: LoadLevel::N0,
            },
            &mut out,
        );
        // Potions: every `UseItem` of this tick that was not refused.
        let sent = pending
            .iter()
            .filter(|c| matches!(c, Command::UseItem { .. }))
            .count();
        let refused = out
            .refused
            .iter()
            .filter(|(p, r)| *p == me && matches!(r, Refusal::NotReady | Refusal::InvalidSlot))
            .count();
        potions += sent.saturating_sub(refused) as u32;
        pending.clear();
        let mine = z
            .world
            .query_mut::<(&Body, &PlayerComp, &Fighter)>()
            .into_iter()
            .find(|(_, (_, p, _))| p.player == me)
            .map(|(_, (b, _, f))| (b.id, b.pos, f.hp, f.max_hp));
        if t % 200 == 0 && mine.is_some() {
            last = z
                .players()
                .into_iter()
                .find(|(p, _, _)| *p == me)
                .map(|(_, s, _)| s);
        }
        // Died for good (past the newbie level): the binary would save and
        // take the character out; it comes back in with a fresh sheet.
        if out.dead.contains(&me) {
            xp += gained(&last);
            target = None;
            pending.push(Command::Leave { player: me });
            continue;
        }
        let Some((my_id, pos, hp, max_hp)) = mine else {
            // Out after a death for good: back in with a fresh sheet.
            last = None;
            target = None;
            pending.push(Command::Enter {
                player: me,
                sheet: Box::new(hunter(b)),
                pos: None,
            });
            continue;
        };
        kills += out
            .combat
            .iter()
            .filter(|c| c.attacker == my_id && c.killed)
            .count() as u32;
        if out.combat.iter().any(|c| c.target == my_id && c.killed) {
            falls += 1;
        }
        // Fallen (under the newbie level): wait to come back in the camp.
        if hp == 0 {
            target = None;
            continue;
        }
        // A potion below a third of the HP (the item's own cooldown applies).
        if t % 10 == 0 && hp * 3 < max_hp {
            // The first stack with potions left; the others are refused.
            pending.extend((0..POTION_STACKS).map(|slot| Command::UseItem { player: me, slot }));
        }
        if t % 20 != 0 {
            continue;
        }
        let at = |z: &Zone, id: EntityId| {
            z.by_id
                .get(&id)
                .and_then(|h| z.world.get::<&Body>(*h).ok().map(|b| b.pos))
        };
        if let Some((id, tpos)) = target.and_then(|id| at(&z, id).map(|p| (id, p))) {
            if tpos.distance(pos) <= 1 {
                pending.extend(techniques.iter().map(|technique| Command::UseTechnique {
                    player: me,
                    technique: *technique,
                    target: id,
                }));
            }
            if t % 100 == 0 {
                // A `Move` or an `Attack` goes at most 32 tiles: walk the
                // path in legs, then attack (again after a fall).
                if tpos.distance(pos) > 24 {
                    if let Some(steps) = world::path(&z.map, pos, tpos, PATH_MAX_NODES * 4) {
                        let leg = steps[steps.len().min(24) - 1];
                        pending.push(Command::Move {
                            player: me,
                            destination: leg,
                        });
                    } else {
                        target = None;
                    }
                } else {
                    pending.push(Command::Attack {
                        player: me,
                        target: id,
                    });
                }
            }
            continue;
        }
        target = None;
        // Rest before the next pull.
        if hp * 10 < max_hp * 9 {
            continue;
        }
        // Pull the nearest prey, preferring one with no other monster near
        // it (a player picks the stray one rather than the pack).
        let monsters: Vec<(EntityId, TilePos, Option<usize>)> = z
            .world
            .query_mut::<(&Body, &MonsterComp)>()
            .into_iter()
            .map(|(_, (b, m))| (b.id, b.pos, prey.iter().position(|p| *p == m.monster)))
            .collect();
        target = monsters
            .iter()
            .filter_map(|(id, p, rank)| Some((*id, *p, (*rank)?)))
            .min_by_key(|(id, p, rank)| {
                let near = monsters
                    .iter()
                    .filter(|(o, q, _)| o != id && q.distance(*p) <= 5)
                    .count();
                // Few neighbors first, then the band's main prey, then distance.
                (near, *rank, p.distance(pos) / 8, *id)
            })
            .map(|(id, _, _)| id);
    }
    // XP from the sheet: the level-ups and what is left, minus the start.
    let sheet = z
        .players()
        .into_iter()
        .find(|(p, _, _)| *p == me)
        .map(|(_, s, _)| s);
    xp += gained(&sheet);
    Result {
        potions: potions * 10 / hours_tenths as u32,
        xp_per_hour: xp * 10 / hours_tenths,
        kills,
        falls,
    }
}

#[test]
#[ignore = "measurement: run with --release -- --ignored --nocapture"]
fn pace_of_the_alpha_map() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
    let mut raw = defs::load(&root).unwrap();
    let _ = raw.fill_missing_text();
    let content = Arc::new(Content::new(raw.validate().unwrap()));
    let f = content.catalog.formulas();
    println!("| zone | level | XP/h | kills/h | falls/h | potions/h | minutes to next level |");
    for b in BANDS {
        let r = hunt(&content, b, HOURS_TENTHS);
        let minutes = f.level_xp(b.level) * 60 / r.xp_per_hour.max(1);
        println!(
            "| {} | {} | {} | {} | {} | {} | {} |",
            b.zone,
            b.level,
            r.xp_per_hour,
            r.kills * 10 / HOURS_TENTHS as u32,
            r.falls * 10 / HOURS_TENTHS as u32,
            r.potions,
            minutes
        );
    }
}
