//! No window lies on another: every mix of open windows, at the screen
//! sizes players use, keeps each window and HUD part in its own place.

use crate::tests::{model, ui};
use crate::*;
use base::Locale;

fn full() -> HudModel {
    let many = |n: usize, s: &str| (0..n).map(|i| format!("{s} {i}")).collect::<Vec<_>>();
    let mut m = model();
    m.fallen = false;
    m.system.clear();
    m.menu = MenuWindow::ALL
        .iter()
        .map(|w| MenuButton {
            window: *w,
            icon: None,
            key: "K".into(),
            alert: false,
        })
        .collect();
    // The real backpack (`sim::BACKPACK_SLOTS`).
    m.bag = vec![None; 20];
    m.sheet = SheetModel {
        free_points: 3,
        titles: many(10, "Title"),
        proficiencies: many(10, "Skill").into_iter().map(|p| (p, 7)).collect(),
        ..SheetModel::default()
    };
    m.quests = (0..8)
        .map(|i| QuestModel {
            name: format!("Quest {i}"),
            description: "A description of the quest.".into(),
            completed: false,
            objectives: vec![("Wolf pelt: 1/5".into(), false)],
            goal: None,
        })
        .collect();
    m.techniques = (0..14)
        .map(|i| TechniqueModel {
            id: base::TechniqueId::new(i),
            name: format!("Tech {i}"),
            level: 1,
            icon: None,
            tooltip: Vec::new(),
            in_bar: false,
        })
        .collect();
    m.help = HelpModel {
        keys: (0..20).map(|i| (format!("Action {i}"), "F1".into())).collect(),
        tips: many(4, "A tip"),
    };
    m
}

fn shop() -> ShopModel {
    ShopModel {
        title: "Forge".into(),
        lines: (0..10)
            .map(|i| ShopLineModel {
                name: format!("Item {i}"),
                icon: None,
                price: 100,
                qty: None,
                affordable: true,
                tooltip: vec![],
                rarity: base::Rarity::Common,
            })
            .collect(),
        repairs: true,
        coins: "You have 900 copper".into(),
        appraises: false,
        purse: None,
        close: "Esc: close".into(),
    }
}

fn trade() -> TradeModel {
    TradeModel {
        with: "Bravo".into(),
        open: true,
        asked: false,
        mine: Vec::new(),
        mine_coins: 0,
        theirs: Vec::new(),
        theirs_coins: 0,
        mine_confirmed: false,
        theirs_confirmed: false,
    }
}

fn party() -> PartyModel {
    PartyModel {
        members: (0..4)
            .map(|i| PartyMember {
                id: base::EntityId::new(i + 1),
                name: format!("Member {i}"),
                leader: i == 0,
                me: i == 0,
                health: Some(100),
            })
            .collect(),
        i_lead: true,
    }
}

/// Every pair of parts that cover each other, once each, with the first
/// screen and mix where it happened.
fn scan() -> Vec<String> {
    let mut found: std::collections::BTreeMap<(&str, &str), String> = Default::default();
    let base = full();
    let variants: Vec<(&str, HudModel)> = {
        let mut v = vec![("plain", base.clone())];
        let mut m = base.clone();
        m.shop = Some(shop());
        v.push(("shop", m));
        let mut m = base.clone();
        m.trade = Some(trade());
        v.push(("trade", m));
        let mut m = base.clone();
        m.dialogue = Some(("The smith looks up.".into(), vec!["Buy".into(), "Bye".into()]));
        v.push(("dialogue", m));
        let mut m = base.clone();
        m.party = Some(party());
        m.invite = Some("Charlie wants to trade.".into());
        v.push(("party+invite", m));
        v
    };
    let mut u = ui(Locale::EN);
    for screen in [
        (1024.0, 600.0),
        (1280.0, 720.0),
        (1366.0, 768.0),
        (1920.0, 1080.0),
    ] {
        let input = Input {
            screen,
            scale: 1.0,
            mouse: (-10.0, -10.0),
            ..Input::default()
        };
        for (name, m) in &variants {
            for bits in 0u32..128 {
                let on = |i: u32| bits & (1 << i) != 0;
                // Status and crafting share the left side: one at a time.
                // So do help and options, in the middle.
                if (on(1) && on(6)) || (on(4) && on(5)) {
                    continue;
                }
                u.hud = HudState::default();
                u.hud.inventory = on(0);
                u.hud.sheet = on(1);
                u.hud.quests = on(2);
                u.hud.techniques = on(3);
                u.hud.help = on(4);
                u.hud.options = on(5);
                u.hud.crafting = on(6);
                // Opened together; the next frame sorts them out and the
                // closed ones fade away.
                for now_ms in [1_000, 2_000, 3_000] {
                    let at = Input {
                        now_ms,
                        ..input.clone()
                    };
                    u.frame(&at, |f, hud| hud.draw(f, m));
                }
                let placed = u.placed.clone();
                for (i, (a, ra)) in placed.iter().enumerate() {
                    for (b, rb) in &placed[i + 1..] {
                        if crate::hud::covers(*ra, *rb) {
                            let key = if a < b { (*a, *b) } else { (*b, *a) };
                            found
                                .entry(key)
                                .or_insert_with(|| format!("{screen:?} {name} bits={bits:07b}"));
                        }
                    }
                }
            }
        }
    }
    found
        .into_iter()
        .map(|((a, b), at)| format!("{a} x {b} @ {at}"))
        .collect()
}

#[test]
fn no_window_lies_on_another() {
    let found = scan();
    assert!(
        found.is_empty(),
        "{} overlaps:\n{}",
        found.len(),
        found.join("\n")
    );
}

fn at(screen: (f32, f32), now_ms: u64) -> Input {
    Input {
        screen,
        scale: 1.0,
        now_ms,
        mouse: (-10.0, -10.0),
        ..Input::default()
    }
}

fn placed(u: &Ui, name: &str) -> bool {
    u.placed.iter().any(|(n, _)| *n == name)
}

#[test]
fn the_newest_window_closes_the_ones_it_covers() {
    let m = full();
    let mut u = ui(Locale::EN);
    let screen = (1280.0, 720.0);
    u.hud.toggle_help();
    for t in [0, 500] {
        u.frame(&at(screen, t), |f, hud| hud.draw(f, &m));
    }
    // The techniques window over the hotbar covers the help at 1280.
    u.hud.toggle_techniques();
    for t in [1_000, 1_100, 2_000] {
        u.frame(&at(screen, t), |f, hud| hud.draw(f, &m));
    }
    assert!(u.hud.techniques, "the newest stays");
    assert!(!u.hud.help, "the older one it covered closed");
    assert!(!placed(&u, "ui.help.title"), "and faded away");
}

#[test]
fn a_window_that_would_cover_the_shop_waits_for_it() {
    let mut m = full();
    m.shop = Some(shop());
    let mut u = ui(Locale::EN);
    let screen = (1280.0, 720.0);
    u.hud.toggle_help();
    for t in [0, 500, 1_000] {
        u.frame(&at(screen, t), |f, hud| hud.draw(f, &m));
    }
    assert!(u.hud.help, "still open");
    assert!(!placed(&u, "ui.help.title"), "but not drawn over the shop");
    assert!(placed(&u, "ui.shop.title"));
    m.shop = None;
    for t in [1_500, 2_000] {
        u.frame(&at(screen, t), |f, hud| hud.draw(f, &m));
    }
    assert!(placed(&u, "ui.help.title"), "the shop gone, it shows");
}

#[test]
fn windows_fade_in_and_out() {
    let mut m = full();
    // No quest tracker fading out in the same corner.
    m.quests.clear();
    let mut u = ui(Locale::EN);
    let screen = (1920.0, 1080.0);
    // Alpha of the brightest vertex drawn for the inventory's panel corner.
    let panel_alpha = |u: &Ui| {
        let r = u
            .placed
            .iter()
            .find(|(n, _)| *n == "ui.inventory.title")
            .map(|(_, r)| *r);
        r.map(|r| {
            u.vertices()
                .iter()
                .filter(|v| r.contains((v.pos[0], v.pos[1] - 0.01)) || r.contains((v.pos[0], v.pos[1])))
                .map(|v| v.color[3])
                .fold(0.0f32, f32::max)
        })
    };
    u.frame(&at(screen, 0), |f, hud| hud.draw(f, &m));
    u.hud.toggle_inventory();
    u.frame(&at(screen, 1_000), |f, hud| hud.draw(f, &m));
    let opening = panel_alpha(&u).expect("drawn at once");
    u.frame(&at(screen, 1_060), |f, hud| hud.draw(f, &m));
    let midway = panel_alpha(&u).unwrap();
    u.frame(&at(screen, 2_000), |f, hud| hud.draw(f, &m));
    let open = panel_alpha(&u).unwrap();
    assert!(opening < midway && midway < open, "{opening} {midway} {open}");
    u.hud.toggle_inventory();
    u.frame(&at(screen, 2_050), |f, hud| hud.draw(f, &m));
    assert!(placed(&u, "ui.inventory.title"), "still fading out");
    u.frame(&at(screen, 3_000), |f, hud| hud.draw(f, &m));
    assert!(!placed(&u, "ui.inventory.title"), "gone");
}
