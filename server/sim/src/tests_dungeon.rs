//! Dungeons: the door checks level and group size; the boss's death clears
//! the run, counts `clears` for everyone inside and earns its titles.

use crate::*;
use base::{CharacterName, PlayerId, TilePos};
use std::sync::Arc;

fn content() -> Arc<Content> {
    let keys = [
        "dungeon.crypt.name",
        "monster.rat.name",
        "proficiency.unarmed.name",
        "title.delver.name",
        "title.delver.desc",
        "system.dungeon_requirements",
        "system.dungeon_cleared",
        "system.title",
        "system.xp",
        "system.level_up",
        "system.points",
        "system.proficiency_up",
        "loot.coins",
        "loot.item",
        "item.relic.name",
        "item.relic.desc",
        "system.unique_lost",
    ];
    let text = format!(
        "{{ {} }}",
        keys.iter()
            .map(|k| format!("\"{k}\": \"x\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let row = |n: usize| format!("\"{}\"", ".".repeat(n));
    let map = |w: usize, h: usize| {
        format!(
            "(legend: {{ '.': (floor: 1) }}, rows: [{}])",
            vec![row(w); h].join(",")
        )
    };
    let files: Vec<(String, String)> = vec![
        ("text/en/a.ron".into(), text),
        (
            "zones/field/zone.ron".into(),
            "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 1, y: 1), starting: true, \
             portals: [(min: (x: 5, y: 1), max: (x: 5, y: 1), to: (x: 101, y: 1))])"
                .into(),
        ),
        ("zones/field/map.ron".into(), map(10, 5)),
        (
            "zones/crypt/zone.ron".into(),
            "(origin: (x: 100, y: 0), view_radius: 20, spawn: (x: 100, y: 3), \
             regions: [(kind: Safe, min: (x: 100, y: 3), max: (x: 100, y: 3))], \
             instance: Some((exit: (x: 2, y: 2))))"
                .into(),
        ),
        ("zones/crypt/map.ron".into(), map(6, 4)),
        (
            "zones/crypt/monsters.ron".into(),
            "[(monster: \"rat\", pos: (x: 102, y: 1))]".into(),
        ),
        (
            "monsters/rat.ron".into(),
            "(level: 1, hp: 1, damage: (0, 0), xp: 1, aggro_radius: 0, leash: 10, loot: [(item: \"relic\", chance: 1000)])".into(),
        ),
        (
            "items/relic.ron".into(),
            "(kind: Armor, equip: Some(Armor(defense: 1)), unique: true)".into(),
        ),
        ("proficiencies/unarmed.ron".into(), "(kind: Weapon)".into()),
        ("counters.ron".into(), "[\"clears\"]".into()),
        (
            "titles/delver.ron".into(),
            "(trigger: Counter(family: \"clears\", key: \"crypt\", at: 1))".into(),
        ),
        (
            "dungeons/crypt.ron".into(),
            "(zone: \"crypt\", party: (1, 1), level: (1, 5), boss: \"rat\", lockout: Daily)".into(),
        ),
    ];
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    Arc::new(Content::new(defs::from_text(&refs).unwrap().validate().unwrap()))
}

fn tick(z: &mut Zone, out: &mut TickOutput, commands: &[Command]) {
    let input = TickInput {
        commands,
        now_unix: 1_000,
        load: LoadLevel::N0,
    };
    z.tick(&input, out);
}

#[test]
fn the_door_checks_the_level_and_the_boss_clears_the_run() {
    let c = content();
    let (field, _) = c.catalog.zone_by_name("field").unwrap();
    let (crypt, _) = c.catalog.zone_by_name("crypt").unwrap();
    let mut f = Zone::new(c.clone(), field, 1).unwrap();
    let mut out = TickOutput::default();
    let p1 = PlayerId::new(1);
    let mut strong = CharacterSheet::new(CharacterName::try_from("strong").unwrap());
    strong.progress.level = 9;
    let enter = |p: u32, sheet: CharacterSheet| Command::Enter {
        player: PlayerId::new(p),
        sheet: Box::new(sheet),
        pos: Some(TilePos::new(4, 1)),
    };
    // Level 9 is too high: told why, still in the field.
    tick(&mut f, &mut out, &[enter(1, strong)]);
    let mut told = false;
    for i in 0..20 {
        let walk = [Command::Move {
            player: p1,
            destination: TilePos::new(5, 1),
        }];
        tick(&mut f, &mut out, if i == 0 { &walk } else { &[] });
        told |= !out.system.is_empty();
        assert!(out.handoffs.is_empty());
    }
    assert!(told);
    tick(&mut f, &mut out, &[Command::Leave { player: p1 }]);

    // Level 1 goes in, to the crypt.
    let hero = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    tick(&mut f, &mut out, &[enter(2, hero)]);
    let p2 = PlayerId::new(2);
    let mut arrival = None;
    for i in 0..20 {
        let walk = [Command::Move {
            player: p2,
            destination: TilePos::new(5, 1),
        }];
        tick(&mut f, &mut out, if i == 0 { &walk } else { &[] });
        if let Some((p, to, a)) = out.handoffs.pop() {
            assert_eq!((p, to), (p2, crypt));
            arrival = Some(a);
            break;
        }
    }
    let arrival = arrival.expect("handed to the crypt");
    let mut k = Zone::new(c.clone(), crypt, 2).unwrap();
    let mut kout = TickOutput::default();
    tick(
        &mut k,
        &mut kout,
        &[
            Command::Arrive { player: p2, arrival },
            Command::ItemIds { start: 10, end: 100 },
        ],
    );
    let (rat, _) = c.catalog.monster_by_name("rat").unwrap();
    let target = k.monster_ids(rat)[0];
    let mut clear = None;
    let mut claims = Vec::new();
    for i in 0..200 {
        let attack = [Command::Attack { player: p2, target }];
        tick(&mut k, &mut kout, if i == 0 { &attack } else { &[] });
        claims.append(&mut kout.claims);
        if let Some(c) = kout.clears.pop() {
            clear = Some(c);
            break;
        }
    }
    let clear = clear.expect("the boss died");
    assert_eq!(
        (clear.dungeon.as_str(), clear.players.as_slice()),
        ("crypt", &[p2][..])
    );
    let (_, sheet, _) = k.players().into_iter().find(|(p, _, _)| *p == p2).unwrap();
    assert!(
        sheet
            .progress
            .counters
            .contains(&("clears".into(), "crypt".into(), 1))
    );
    assert_eq!(sheet.progress.titles, vec!["delver".to_owned()]);
    // The relic dropped and is claimed for the server.
    assert_eq!(claims, vec![(p2, "item:relic".to_owned())]);
    assert!(sheet.items.iter().any(|i| i.item == "relic"));
    // Someone else's piece came first: this one goes.
    tick(
        &mut k,
        &mut kout,
        &[Command::Revoke {
            player: p2,
            key: "item:relic".into(),
        }],
    );
    let (_, sheet, _) = k.players().into_iter().find(|(p, _, _)| *p == p2).unwrap();
    assert!(!sheet.items.iter().any(|i| i.item == "relic"));
    // Falling inside (a newbie) comes back outside, at the exit.
    let h = k.by_player[&p2];
    k.world.get::<&mut fight::Fighter>(h).unwrap().hp = 0;
    fight::die(&mut k, h, &mut kout);
    let (who, to, a) = kout.handoffs.pop().expect("sent out");
    assert_eq!(
        (who, to, a.pos),
        (p2, c.catalog.zone_by_name("field").unwrap().0, TilePos::new(2, 2))
    );
}
