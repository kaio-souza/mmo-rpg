//! Tick cost with N players walking at random on an open 256x256 map.
//! `cargo run --release -p sim --example tick_bench -- 1000 [content-dir zone]`
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

use base::{CharacterName, PlayerId, TilePos, ZoneId};
use sim::*;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let n: u32 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1000);
    let side = 256;
    let row = ".".repeat(side);
    let rows: Vec<String> = (0..side).map(|_| format!("\"{row}\"")).collect();
    let map = format!("(legend: {{ '.': (floor: 1) }}, rows: [{}])", rows.join(","));
    let text = r#"{ "system.ok": "x" }"#;
    let mut files: Vec<(String, String)> = [i18n::Locale::EN]
        .iter()
        .map(|l| (format!("text/{l}/a.ron"), text.to_owned()))
        .collect();
    files.push((
        "zones/b/zone.ron".into(),
        "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 128, y: 128))".into(),
    ));
    files.push(("zones/b/map.ron".into(), map));
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let (cat, zid) = match (std::env::args().nth(2), std::env::args().nth(3)) {
        (Some(dir), Some(name)) => {
            let c = Arc::new(
                defs::load(std::path::Path::new(&dir))
                    .unwrap()
                    .validate()
                    .unwrap(),
            );
            let id = c.zone_by_name(&name).unwrap().0;
            (c, id)
        }
        _ => (
            Arc::new(defs::from_text(&refs).unwrap().validate().unwrap()),
            ZoneId::new(0),
        ),
    };
    let origin = cat.zone(zid).unwrap().origin;
    let content = Arc::new(sim::Content::new(Arc::unwrap_or_clone(cat)));
    let mut zone = Zone::new(content, zid, 1).unwrap();
    let mut out = TickOutput::default();

    let mut rng = 0x2545_f491_4f6c_dd1du64;
    let mut next = move |m: u32| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % u64::from(m)) as i32
    };
    let enters: Vec<Command> = (0..n)
        .map(|i| Command::Enter {
            player: PlayerId::new(i),
            sheet: Box::new(CharacterSheet::new(
                CharacterName::try_from(format!("bot{i}")).unwrap(),
            )),
            pos: Some(TilePos::new(
                origin.x + next(side as u32),
                origin.y + next(side as u32),
            )),
        })
        .collect();
    zone.tick(
        &TickInput {
            commands: &enters,
            now_unix: 0,
            load: LoadLevel::N0,
        },
        &mut out,
    );

    let ticks = 400;
    let mut worst = 0u128;
    let mut total = 0u128;
    let mut cmds = Vec::new();
    for t in 0..ticks {
        cmds.clear();
        for i in 0..n {
            if (i + t) % 40 == 0 {
                let pos = zone.position_of(PlayerId::new(i)).unwrap();
                let d = TilePos::new(pos.x + next(21) - 10, pos.y + next(21) - 10);
                cmds.push(Command::Move {
                    player: PlayerId::new(i),
                    destination: d,
                });
            }
        }
        let start = Instant::now();
        zone.tick(
            &TickInput {
                commands: &cmds,
                now_unix: 0,
                load: LoadLevel::N0,
            },
            &mut out,
        );
        let us = start.elapsed().as_micros();
        worst = worst.max(us);
        total += us;
    }
    println!(
        "{n} players, {ticks} ticks: mean {} us, worst {} us",
        total / ticks as u128,
        worst
    );
}
