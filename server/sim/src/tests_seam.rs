//! Phase 10: two zones side by side, the seam between them
//! Each zone runs alone here; the
//! test carries ghosts and handoffs between them like the binary does.

use crate::*;
use base::{CharacterName, EntityId, PlayerId, TilePos, ZoneId};
use std::collections::BTreeMap;
use std::sync::Arc;

fn row(n: usize) -> String {
    format!("\"{}\"", ".".repeat(n))
}

/// Zones `a` (x 0..40) and `b` (x 40..80), ten rows each, all open.
fn content() -> Arc<Content> {
    let text = r#"{ "system.ok": "x" }"#;
    let mut files: Vec<(String, String)> = [i18n::Locale::EN]
        .iter()
        .map(|l| (format!("text/{l}/a.ron"), text.to_owned()))
        .collect();
    let map = format!(
        "(legend: {{ '.': (floor: 1) }}, rows: [{}])",
        vec![row(40); 10].join(",")
    );
    files.push((
        "zones/a/zone.ron".into(),
        "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 5, y: 5), starting: true)".into(),
    ));
    files.push(("zones/a/map.ron".into(), map.clone()));
    files.push((
        "zones/b/zone.ron".into(),
        "(origin: (x: 40, y: 0), view_radius: 20, spawn: (x: 75, y: 5))".into(),
    ));
    files.push(("zones/b/map.ron".into(), map));
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    Arc::new(Content::new(defs::from_text(&refs).unwrap().validate().unwrap()))
}

struct World {
    a: Zone,
    b: Zone,
    out_a: TickOutput,
    out_b: TickOutput,
    /// What each zone last sent the other (the binary keeps this).
    sent_a: Vec<EntityState>,
    sent_b: Vec<EntityState>,
}

const A: ZoneId = ZoneId::new(0);
const B: ZoneId = ZoneId::new(1);

impl World {
    fn new() -> World {
        let c = content();
        World {
            a: Zone::new(c.clone(), A, 1).unwrap(),
            b: Zone::new(c, B, 2).unwrap(),
            out_a: TickOutput::default(),
            out_b: TickOutput::default(),
            sent_a: Vec::new(),
            sent_b: Vec::new(),
        }
    }

    /// One tick of both, with what the other one produced last tick.
    fn tick(&mut self, mut to_a: Vec<Command>, mut to_b: Vec<Command>, link: bool) {
        if link {
            for (to, list) in &self.out_a.border {
                assert_eq!(*to, B);
                let diff = GhostDiff::between(&self.sent_a, list);
                self.sent_a = list.clone();
                to_b.push(Command::Ghosts { from: A, diff });
            }
            for (to, list) in &self.out_b.border {
                assert_eq!(*to, A);
                let diff = GhostDiff::between(&self.sent_b, list);
                self.sent_b = list.clone();
                to_a.push(Command::Ghosts { from: B, diff });
            }
            for (player, to, arrival) in self.out_a.handoffs.drain(..) {
                assert_eq!(to, B);
                to_b.push(Command::Arrive { player, arrival });
            }
            for (player, to, arrival) in self.out_b.handoffs.drain(..) {
                assert_eq!(to, A);
                to_a.push(Command::Arrive { player, arrival });
            }
        }
        fn input(c: &[Command]) -> TickInput<'_> {
            TickInput {
                commands: c,
                now_unix: 1_700_000_000,
                load: LoadLevel::N0,
            }
        }
        self.a.tick(&input(&to_a), &mut self.out_a);
        self.b.tick(&input(&to_b), &mut self.out_b);
    }
}

fn enter(player: u32, name: &str, pos: TilePos) -> Command {
    Command::Enter {
        player: PlayerId::new(player),
        sheet: Box::new(CharacterSheet::new(CharacterName::try_from(name).unwrap())),
        pos: Some(pos),
    }
}

fn walk(player: u32, x: i32, y: i32) -> Command {
    Command::Move {
        player: PlayerId::new(player),
        destination: TilePos::new(x, y),
    }
}

#[test]
fn entity_ids_are_unique_in_the_world() {
    let w = World::new();
    assert_eq!(w.a.neighbors(), vec![B]);
    assert_eq!(w.b.neighbors(), vec![A]);
    let mut w = w;
    w.tick(
        vec![enter(1, "alice", TilePos::new(5, 5))],
        vec![enter(2, "bob", TilePos::new(75, 5))],
        true,
    );
    assert_eq!(w.out_a.entered[0].1, EntityId::new(1));
    assert_eq!(w.out_b.entered[0].1, EntityId::new((1 << ENTITY_ID_BITS) + 1));
}

#[test]
fn the_map_holds_the_neighbor_band() {
    let w = World::new();
    let map = w.a.map();
    assert_eq!(map.origin(), TilePos::new(-24, -24));
    assert!(!map.blocks_movement(TilePos::new(60, 5)), "b's tiles, 21 into it");
    assert!(map.blocks_movement(TilePos::new(-3, 5)), "no zone there");
}

#[test]
fn a_player_crosses_the_seam_and_keeps_walking() {
    let mut w = World::new();
    w.tick(vec![enter(1, "alice", TilePos::new(30, 5))], vec![], true);
    let me = w.out_a.entered[0].1;
    // Both sides are up (ghost heartbeats) before the walk.
    for _ in 0..3 {
        w.tick(vec![], vec![], true);
    }
    w.tick(vec![walk(1, 55, 5)], vec![], true);
    let mut crossed = None;
    for _ in 0..200 {
        w.tick(vec![], vec![], true);
        if let Some((p, to, arr)) = w.out_a.handoffs.first() {
            crossed = Some((*p, *to, arr.pos, arr.entity, arr.destination));
        }
        if w.out_b.entered.iter().any(|(p, _, _)| *p == PlayerId::new(1)) {
            break;
        }
    }
    let (player, to, pos, entity, destination) = crossed.expect("a handed the player over");
    assert_eq!((player, to, entity), (PlayerId::new(1), B, me));
    assert_eq!(pos.x, 40 + 1, "two tiles past the edge (hysteresis)");
    assert_eq!(destination, Some(TilePos::new(55, 5)));
    assert_eq!(w.a.player_count(), 0);
    assert_eq!(w.out_b.entered[0].1, me, "same entity id on the other side");
    for _ in 0..200 {
        w.tick(vec![], vec![], true);
    }
    assert_eq!(w.b.position_of(PlayerId::new(1)), Some(TilePos::new(55, 5)));
}

/// The crossing does not show: the step onto the next zone reaches the
/// player from the zone it leaves, on its tick, and the next zone keeps the
/// pace (no pause, no step on arrival).
#[test]
fn the_walk_keeps_its_pace_across_the_seam() {
    let mut w = World::new();
    w.tick(vec![enter(1, "alice", TilePos::new(30, 5))], vec![], true);
    let me = w.out_a.entered[0].1;
    for _ in 0..3 {
        w.tick(vec![], vec![], true);
    }
    w.tick(vec![walk(1, 55, 5)], vec![], true);
    let p = PlayerId::new(1);
    let mut last = w.a.position_of(p);
    let mut steps = Vec::new();
    let mut seen_crossing = false;
    for t in 0..200u32 {
        w.tick(vec![], vec![], true);
        let mut in_transit = None;
        if let Some((_, _, arr)) = w.out_a.handoffs.first() {
            in_transit = Some(arr.pos);
            let sent = w
                .out_a
                .views
                .iter()
                .filter(|(who, _)| *who == p)
                .flat_map(|(_, v)| &v.moved)
                .any(|(id, pos, _)| *id == me && *pos == arr.pos);
            assert!(sent, "the crossing step goes out with the handoff tick");
            seen_crossing = true;
        }
        // On the handoff tick the player is in neither zone, in transit.
        let now = w.a.position_of(p).or_else(|| w.b.position_of(p)).or(in_transit);
        if now.is_some() && now != last {
            steps.push(t);
            last = now;
        }
        if now == Some(TilePos::new(55, 5)) {
            break;
        }
    }
    assert!(seen_crossing);
    let gaps: Vec<u32> = steps.windows(2).map(|s| s[1] - s[0]).collect();
    assert!(
        gaps.iter().all(|g| *g == gaps[0]),
        "one pace before, across and after the seam: {gaps:?}"
    );
}

#[test]
fn a_silent_neighbor_closes_its_tiles() {
    let mut w = World::new();
    w.tick(vec![enter(1, "alice", TilePos::new(30, 5))], vec![], false);
    w.tick(vec![walk(1, 55, 5)], vec![], false);
    for _ in 0..200 {
        w.tick(vec![], vec![], false);
    }
    assert_eq!(w.a.position_of(PlayerId::new(1)), Some(TilePos::new(39, 5)));
    assert!(w.out_a.handoffs.is_empty());
}

#[test]
fn players_see_across_the_seam_through_ghosts() {
    let mut w = World::new();
    w.tick(
        vec![enter(1, "alice", TilePos::new(35, 5))],
        vec![enter(2, "bob", TilePos::new(45, 5))],
        true,
    );
    let bob = w.out_b.entered[0].1;
    w.tick(vec![], vec![], true);
    w.tick(vec![], vec![], true);
    assert!(w.a.knows(PlayerId::new(1), bob), "alice sees bob's ghost");
    // Bob walks away from the seam: the ghost leaves alice's view.
    w.tick(vec![], vec![walk(2, 75, 5)], true);
    for _ in 0..300 {
        w.tick(vec![], vec![], true);
    }
    assert!(!w.a.knows(PlayerId::new(1), bob));
}

#[test]
fn ghosts_expire_when_the_neighbor_goes_quiet() {
    let mut w = World::new();
    w.tick(
        vec![enter(1, "alice", TilePos::new(35, 5))],
        vec![enter(2, "bob", TilePos::new(45, 5))],
        true,
    );
    let bob = w.out_b.entered[0].1;
    w.tick(vec![], vec![], true);
    w.tick(vec![], vec![], true);
    assert!(w.a.knows(PlayerId::new(1), bob));
    for _ in 0..(GHOST_TIMEOUT_TICKS + 2) {
        w.tick(vec![], vec![], false);
    }
    assert!(!w.a.knows(PlayerId::new(1), bob));
}

#[test]
fn the_same_commands_give_the_same_world() {
    let run = || {
        let mut w = World::new();
        w.tick(
            vec![enter(1, "alice", TilePos::new(30, 5))],
            vec![enter(2, "bob", TilePos::new(45, 5))],
            true,
        );
        w.tick(vec![walk(1, 50, 5)], vec![walk(2, 20, 6)], true);
        for _ in 0..120 {
            w.tick(vec![], vec![], true);
        }
        (w.a.state_hash(), w.b.state_hash())
    };
    assert_eq!(run(), run());
}

#[test]
fn ghost_diff_sends_only_changes() {
    let mut w = World::new();
    w.tick(vec![enter(1, "alice", TilePos::new(35, 5))], vec![], true);
    let first = w.out_a.border[0].1.clone();
    assert_eq!(first.len(), 1);
    let d = GhostDiff::between(&[], &first);
    assert_eq!(d.upsert.len(), 1);
    assert!(GhostDiff::between(&first, &first).is_empty());
    let d = GhostDiff::between(&first, &[]);
    assert_eq!(d.gone, vec![first[0].id]);
}

/// A wall along x = 40 with a gate four tiles wide; `split`: the wall is
/// the first column of zone `b`, otherwise one zone holds both halves.
fn gate_content(split: bool) -> Arc<Content> {
    let text = r#"{ "system.ok": "x" }"#;
    let mut files: Vec<(String, String)> = [i18n::Locale::EN]
        .iter()
        .map(|l| (format!("text/{l}/a.ron"), text.to_owned()))
        .collect();
    let rows = |w: usize, wall: Option<usize>| -> String {
        (0..10)
            .map(|y| {
                let r: String = (0..w)
                    .map(|x| {
                        if Some(x) == wall && !(3..7).contains(&y) {
                            '#'
                        } else {
                            '.'
                        }
                    })
                    .collect();
                format!("\"{r}\"")
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let legend = "legend: { '.': (floor: 1), '#': (floor: 3, blocks_movement: true, blocks_sight: true) }";
    if split {
        files.push((
            "zones/a/zone.ron".into(),
            "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 5, y: 5), starting: true)".into(),
        ));
        files.push((
            "zones/a/map.ron".into(),
            format!("({legend}, rows: [{}])", rows(40, None)),
        ));
        files.push((
            "zones/b/zone.ron".into(),
            "(origin: (x: 40, y: 0), view_radius: 20, spawn: (x: 75, y: 5))".into(),
        ));
        files.push((
            "zones/b/map.ron".into(),
            format!("({legend}, rows: [{}])", rows(40, Some(0))),
        ));
    } else {
        files.push((
            "zones/a/zone.ron".into(),
            "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 5, y: 5))".into(),
        ));
        files.push((
            "zones/a/map.ron".into(),
            format!("({legend}, rows: [{}])", rows(80, Some(40))),
        ));
    }
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    Arc::new(Content::new(defs::from_text(&refs).unwrap().validate().unwrap()))
}

/// `n` players walk back and forth between x = 25 and x = 55 for `ticks`,
/// across whatever zones the content has; the legs each one finished.
fn crowd(c: Arc<Content>, n: u32, ticks: u32) -> Vec<u32> {
    let start = |i: u32| TilePos::new(25 + (i % 10) as i32, (i / 10) as i32 * 3 + 1);
    let goal = |_: u32, pos: TilePos, there: bool| TilePos::new(if there { 55 } else { 25 }, pos.y);
    walk_crowd(c, ZoneId::new(0), n, ticks, &start, &goal)
}

/// The crowd walk: each player enters at `start(i)` in zone `first`, then
/// walks to `goal(i, pos, true)`, back to `goal(i, pos, false)`, and so on,
/// ordered every 60 ticks like a client would. Legs finished per player.
fn walk_crowd(
    c: Arc<Content>,
    first: ZoneId,
    n: u32,
    ticks: u32,
    start: &dyn Fn(u32) -> TilePos,
    goal: &dyn Fn(u32, TilePos, bool) -> TilePos,
) -> Vec<u32> {
    let ids: Vec<ZoneId> = c.catalog.zones().map(|(id, _)| id).collect();
    let mut zones: Vec<Zone> = ids
        .iter()
        .map(|id| Zone::new(c.clone(), *id, 1).unwrap())
        .collect();
    let mut outs: Vec<TickOutput> = ids.iter().map(|_| TickOutput::default()).collect();
    let mut sent: BTreeMap<(ZoneId, ZoneId), Vec<EntityState>> = BTreeMap::new();
    let mut inbox: Vec<Vec<Command>> = ids.iter().map(|_| Vec::new()).collect();
    let k0 = ids.iter().position(|z| *z == first).unwrap();
    inbox[k0] = (0..n)
        .map(|i| enter(i + 1, &format!("walker{i}"), start(i)))
        .collect();
    let mut there = vec![true; n as usize];
    let mut legs = vec![0u32; n as usize];
    let at = |zones: &[Zone], p: PlayerId| zones.iter().position(|z| z.position_of(p).is_some());
    for t in 0..ticks {
        if t % 60 == 30 {
            for i in 0..n {
                let p = PlayerId::new(i + 1);
                let Some(k) = at(&zones, p) else { continue };
                let pos = zones[k].position_of(p).unwrap();
                if pos.distance(goal(i, pos, there[i as usize])) <= 1 {
                    there[i as usize] = !there[i as usize];
                    legs[i as usize] += 1;
                }
                let g = goal(i, pos, there[i as usize]);
                inbox[k].push(walk(i + 1, g.x, g.y));
            }
        }
        for (k, z) in zones.iter_mut().enumerate() {
            let cmds = std::mem::take(&mut inbox[k]);
            z.tick(
                &TickInput {
                    commands: &cmds,
                    now_unix: 1_700_000_000,
                    load: LoadLevel::N0,
                },
                &mut outs[k],
            );
        }
        for (k, out) in outs.iter_mut().enumerate() {
            for (to, list) in &out.border {
                let old = sent.entry((ids[k], *to)).or_default();
                let diff = GhostDiff::between(old, list);
                *old = list.clone();
                let j = ids.iter().position(|x| x == to).unwrap();
                inbox[j].push(Command::Ghosts { from: ids[k], diff });
            }
            for (player, to, arrival) in out.handoffs.drain(..) {
                let j = ids.iter().position(|x| *x == to).unwrap();
                inbox[j].push(Command::Arrive { player, arrival });
            }
        }
    }
    for i in 0..n {
        let p = PlayerId::new(i + 1);
        // A handoff on the last tick is still in flight: it counts.
        let in_flight = inbox
            .iter()
            .flatten()
            .filter(|c| matches!(c, Command::Arrive { player, .. } if *player == p))
            .count();
        assert_eq!(
            zones.iter().filter(|z| z.position_of(p).is_some()).count() + in_flight,
            1,
            "player {i} in one zone"
        );
    }
    legs
}

#[test]
fn a_crowd_goes_back_and_forth_over_the_seam() {
    let legs = crowd(content(), 30, 4000);
    assert!(legs.iter().all(|l| *l >= 3), "{legs:?}");
}

#[test]
fn a_gate_on_the_seam_flows_like_one_inside_a_zone() {
    let one = crowd(gate_content(false), 30, 4000);
    let two = crowd(gate_content(true), 30, 4000);
    let (a, b): (u32, u32) = (one.iter().sum(), two.iter().sum());
    assert!(two.iter().all(|l| *l >= 2), "nobody stuck at the seam: {two:?}");
    assert!(b * 10 >= a * 7, "the seam gate keeps 70% of the flow: {b} vs {a}");
}

#[test]
fn an_arrival_on_a_crowded_edge_stays_on_this_side() {
    let mut w = World::new();
    // Bob and friends stand on b's first columns; alice arrives on x = 41.
    let crowd: Vec<Command> = (0..10)
        .flat_map(|y| [40, 41, 42].map(|x| (x, y)))
        .enumerate()
        .map(|(i, (x, y))| enter(10 + i as u32, &format!("stander{i}"), TilePos::new(x, y)))
        .collect();
    w.tick(vec![], crowd, true);
    let mut sheet = CharacterSheet::new(CharacterName::try_from("alice").unwrap());
    sheet.hp = 0;
    let arrival = Arrival {
        entity: EntityId::new(77),
        sheet,
        pos: TilePos::new(41, 5),
        facing: base::Direction::E,
        destination: None,
        step_wait: 0,
        vigor: 0,
        mp: 0,
    };
    w.tick(
        vec![],
        vec![Command::Arrive {
            player: PlayerId::new(1),
            arrival: Box::new(arrival),
        }],
        true,
    );
    let pos = w.b.position_of(PlayerId::new(1)).expect("arrived");
    assert!(pos.x >= 40, "placed on b's own tiles, not on a's: {pos:?}");
    w.tick(vec![], vec![], true);
    assert!(w.out_b.handoffs.is_empty(), "no bounce back");
}

/// The real start: Aurelia's south gate onto the field, crowded both ways.
#[test]
fn a_crowd_walks_through_aurelias_south_gate() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
    let mut raw = defs::load(&root).unwrap();
    let _ = raw.fill_missing_text();
    let c = Arc::new(Content::new(raw.validate().unwrap()));
    let aurelia = c.catalog.zone_by_name("aurelia").unwrap().0;
    let start = |i: u32| TilePos::new(26 + (i % 12) as i32, -20 + (i / 12) as i32 * 2);
    let goal =
        |i: u32, _: TilePos, south: bool| TilePos::new(30 + (i % 4) as i32, if south { 8 } else { -5 });
    let legs = walk_crowd(c, aurelia, 40, 6000, &start, &goal);
    assert!(legs.iter().all(|l| *l >= 3), "{legs:?}");
}

/// Alpha playtest: the spider silk work repeated without a pause. A
/// repeatable quest with `cooldown_secs` starts again only after it.
#[test]
fn a_repeatable_quest_waits_for_its_cooldown() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
    let mut raw = defs::load(&root).unwrap();
    let _ = raw.fill_missing_text();
    let cat = raw.validate().unwrap();
    let (silk, def) = cat.quest_by_name("spider_silk").unwrap();
    assert!(def.repeatable && def.cooldown_secs > 0);
    let (item, _) = cat.item_by_name("spider_silk").unwrap();
    let sheet = CharacterSheet::new(CharacterName::try_from("alice").unwrap());
    let mut data = crate::player::PlayerData::import(&sheet, &cat);
    let t0 = 1_700_000_000;
    data.start(&cat, silk, t0).unwrap();
    data.give(&cat, item, 4).unwrap();
    data.complete(&cat, silk, t0).unwrap();
    let wait = u64::from(def.cooldown_secs);
    assert_eq!(
        data.status(&cat, silk, t0 + wait - 1),
        defs::QuestStatus::Completed
    );
    assert!(data.start(&cat, silk, t0 + wait - 1).is_err(), "too soon");
    assert_eq!(data.status(&cat, silk, t0 + wait), defs::QuestStatus::NotStarted);
    data.start(&cat, silk, t0 + wait).unwrap();
    // The sheet keeps the time across a save.
    let mut saved = sheet.clone();
    data.export(&cat, &mut saved);
    assert_eq!(saved.quests[0].completed_at, t0);
}

/// When the cooldown of a repeatable quest ends, the player hears of it
/// (log and markers) without doing anything: the `!` comes back on its own.
#[test]
fn the_end_of_a_cooldown_refreshes_markers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
    let mut raw = defs::load(&root).unwrap();
    let _ = raw.fill_missing_text();
    let cat = raw.validate().unwrap();
    let wait = u64::from(cat.quest_by_name("spider_silk").unwrap().1.cooldown_secs);
    let (zone, _) = cat.zone_by_name("aurelia").unwrap();
    let content = Arc::new(Content::new(cat));
    let mut z = Zone::new(content, zone, 1).unwrap();
    let t0 = 1_700_000_000;
    let mut sheet = CharacterSheet::new(CharacterName::try_from("alice").unwrap());
    sheet.quests = vec![SavedQuest {
        quest: "spider_silk".into(),
        completed: true,
        talked: 0,
        completed_at: t0,
    }];
    let me = PlayerId::new(1);
    let mut out = TickOutput::default();
    let at = |z: &mut Zone, now: u64, cmds: &[Command], out: &mut TickOutput| {
        z.tick(
            &TickInput {
                commands: cmds,
                now_unix: now,
                load: LoadLevel::N0,
            },
            out,
        );
        out.markers.iter().any(|(p, _)| *p == me)
    };
    let enter = [Command::Enter {
        player: me,
        sheet: Box::new(sheet),
        pos: None,
    }];
    at(&mut z, t0 + wait - 2, &enter, &mut out);
    assert!(!at(&mut z, t0 + wait - 1, &[], &mut out), "nothing changed yet");
    assert!(at(&mut z, t0 + wait, &[], &mut out), "the cooldown ended");
    assert!(!at(&mut z, t0 + wait + 1, &[], &mut out), "only once");
}

/// Alpha pace test: a player inside Aurelia's walls who clicks a rat
/// outside the gate walks out and fights it (was refused on safe ground).
#[test]
fn an_attack_from_safe_ground_walks_out_first() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content");
    let mut raw = defs::load(&root).unwrap();
    let _ = raw.fill_missing_text();
    let cat = raw.validate().unwrap();
    let (zone, _) = cat.zone_by_name("aurelia").unwrap();
    let (rat, _) = cat.monster_by_name("field_rat").unwrap();
    let mut z = Zone::new(Arc::new(Content::new(cat)), zone, 1).unwrap();
    let me = PlayerId::new(1);
    let mut out = TickOutput::default();
    let tick = |z: &mut Zone, cmds: &[Command], out: &mut TickOutput| {
        z.tick(
            &TickInput {
                commands: cmds,
                now_unix: 1_700_000_000,
                load: LoadLevel::N0,
            },
            out,
        );
    };
    // Inside the walls, on the avenue just north of the south gate.
    tick(&mut z, &[enter(1, "alice", TilePos::new(31, -31))], &mut out);
    let target = *z.monster_ids(rat).first().unwrap();
    tick(&mut z, &[Command::Attack { player: me, target }], &mut out);
    assert!(out.refused.is_empty(), "{:?}", out.refused);
    let mut fought = false;
    for _ in 0..1200 {
        tick(&mut z, &[], &mut out);
        if out
            .combat
            .iter()
            .any(|c| c.target == target || c.attacker == target)
        {
            fought = true;
            break;
        }
    }
    assert!(fought, "the player walked out and fought");
}
