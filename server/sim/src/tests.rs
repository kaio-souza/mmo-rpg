use crate::*;
use base::{CharacterName, PlayerId, TilePos, ZoneId};
use std::sync::Arc;

/// 12x7 map: wall column at x = 6 with a gap at y = 6.
const MAP: &str = r#####"(
    legend: { '.': (floor: 1), '#': (floor: 3, blocks_movement: true, blocks_sight: true) },
    rows: [
        "......#.....",
        "......#.....",
        "......#.....",
        "......#.....",
        "......#.....",
        "......#.....",
        "............",
    ],
)"#####;

fn catalog() -> Arc<Content> {
    let text = r#"{ "system.ok": "x" }"#;
    let mut files: Vec<(String, String)> = [i18n::Locale::EN]
        .iter()
        .map(|l| (format!("text/{l}/a.ron"), text.to_owned()))
        .collect();
    files.push((
        "zones/test/zone.ron".into(),
        "(origin: (x: 100, y: 100), view_radius: 20, spawn: (x: 101, y: 101))".into(),
    ));
    files.push(("zones/test/map.ron".into(), MAP.into()));
    content_from(files)
}

fn content_from(files: Vec<(String, String)>) -> Arc<Content> {
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    Arc::new(Content::new(defs::from_text(&refs).unwrap().validate().unwrap()))
}

fn p(x: i32, y: i32) -> TilePos {
    TilePos::new(100 + x, 100 + y)
}

fn enter(player: u32, name: &str, pos: TilePos) -> Command {
    Command::Enter {
        player: PlayerId::new(player),
        sheet: Box::new(CharacterSheet::new(CharacterName::try_from(name).unwrap())),
        pos: Some(pos),
    }
}

struct Run {
    zone: Zone,
    out: TickOutput,
}

impl Run {
    fn new() -> Self {
        Self {
            zone: Zone::new(catalog(), ZoneId::new(0), 42).unwrap(),
            out: TickOutput::default(),
        }
    }

    fn tick(&mut self, commands: &[Command]) -> &TickOutput {
        let input = TickInput {
            commands,
            now_unix: 1_700_000_000,
            load: LoadLevel::N0,
        };
        self.zone.tick(&input, &mut self.out);
        &self.out
    }

    fn view(&self, player: u32) -> Option<&View> {
        self.out
            .views
            .iter()
            .find(|(p, _)| *p == PlayerId::new(player))
            .map(|(_, v)| v)
    }
}

#[test]
fn players_see_each_other_and_leave() {
    let mut r = Run::new();
    let out = r.tick(&[enter(1, "alice", p(1, 1)), enter(2, "bob", p(3, 1))]);
    assert_eq!(out.entered.len(), 2);
    assert_eq!(out.entities.len(), 2);
    assert_eq!(r.view(1).unwrap().entered.len(), 2);
    assert_eq!(r.view(2).unwrap().entered.len(), 2);

    r.tick(&[]);
    assert!(r.out.views.is_empty(), "nothing changed");

    let out = r.tick(&[Command::Leave {
        player: PlayerId::new(2),
    }]);
    assert_eq!(out.left.len(), 1);
    assert_eq!(out.left[0].2, p(3, 1), "leaving reports the last position");
    assert_eq!(r.view(1).unwrap().left.len(), 1);
}

#[test]
fn entering_on_a_taken_tile_goes_next_to_it() {
    let mut r = Run::new();
    let out = r.tick(&[enter(1, "alice", p(1, 1)), enter(2, "bob", p(1, 1))]);
    let bob = out.entered[1].2;
    assert_ne!(bob, p(1, 1));
    assert!(bob.distance(p(1, 1)) <= 1, "{bob:?}");
}

#[test]
fn players_lists_everyone_sorted() {
    let mut r = Run::new();
    r.tick(&[enter(5, "eve", p(2, 2)), enter(1, "alice", p(1, 1))]);
    let all = r.zone.players();
    let ids: Vec<u32> = all.iter().map(|(id, _, _)| id.get()).collect();
    assert_eq!(ids, vec![1, 5]);
    assert_eq!(all[1].2, p(2, 2));
    assert_eq!(all[1].1.name.as_str(), "eve");
}

#[test]
fn wall_blocks_sight_until_walking_around() {
    let mut r = Run::new();
    r.tick(&[enter(1, "alice", p(4, 2)), enter(2, "bob", p(8, 2))]);
    assert_eq!(r.view(1).unwrap().entered.len(), 1, "only itself");

    // Walk alice through the gap at (6, 6) to (8, 5).
    r.tick(&[Command::Move {
        player: PlayerId::new(1),
        destination: p(8, 5),
    }]);
    let mut saw_bob = false;
    for _ in 0..40 {
        r.tick(&[]);
        if let Some(v) = r.view(1)
            && !v.entered.is_empty()
        {
            saw_bob = true;
        }
    }
    assert!(saw_bob);
    assert_eq!(r.zone.position_of(PlayerId::new(1)), Some(p(8, 5)));
}

#[test]
fn walking_speed_and_moved_views() {
    let mut r = Run::new();
    r.tick(&[enter(1, "alice", p(0, 0)), enter(2, "bob", p(0, 3))]);
    r.tick(&[Command::Move {
        player: PlayerId::new(1),
        destination: p(3, 0),
    }]);
    // First step at once, then one every STEP_TICKS.
    assert_eq!(r.zone.position_of(PlayerId::new(1)), Some(p(1, 0)));
    assert_eq!(r.view(2).unwrap().moved.len(), 1);
    for _ in 0..STEP_TICKS {
        r.tick(&[]);
    }
    assert_eq!(r.zone.position_of(PlayerId::new(1)), Some(p(2, 0)));
    // Re-sending Move does not make the next step sooner.
    r.tick(&[Command::Move {
        player: PlayerId::new(1),
        destination: p(3, 0),
    }]);
    assert_eq!(r.zone.position_of(PlayerId::new(1)), Some(p(2, 0)));
}

#[test]
fn a_step_along_the_screen_horizontal_takes_longer() {
    let mut r = Run::new();
    r.tick(&[enter(1, "alice", p(0, 5))]);
    r.tick(&[Command::Move {
        player: PlayerId::new(1),
        destination: p(2, 3),
    }]);
    // NE crosses twice the screen width of a straight step: 7 ticks, not 4.
    assert_eq!(r.zone.position_of(PlayerId::new(1)), Some(p(1, 4)));
    for _ in 0..base::WIDE_STEP_TICKS - 1 {
        r.tick(&[]);
    }
    assert_eq!(r.zone.position_of(PlayerId::new(1)), Some(p(1, 4)));
    r.tick(&[]);
    assert_eq!(r.zone.position_of(PlayerId::new(1)), Some(p(2, 3)));
}

#[test]
fn far_move_and_blocked_enter_are_ignored() {
    let mut r = Run::new();
    // A wall (the map changed since the save): the nearest free tile.
    r.tick(&[enter(1, "alice", p(6, 0))]);
    let at = r.zone.position_of(PlayerId::new(1)).unwrap();
    assert_eq!(at.distance(p(6, 0)), 1);
    assert!(!r.zone.map().blocks_movement(at));
    r.tick(&[Command::Move {
        player: PlayerId::new(1),
        destination: TilePos::new(at.x, at.y + MAX_MOVE_DISTANCE as i32 + 1),
    }]);
    assert_eq!(r.zone.position_of(PlayerId::new(1)), Some(at));
}

fn move_to(player: u32, to: TilePos) -> Command {
    Command::Move {
        player: PlayerId::new(player),
        destination: to,
    }
}

#[test]
fn one_being_per_tile() {
    let mut r = Run::new();
    // Same tile asked twice: the second goes to a free tile nearby.
    r.tick(&[enter(1, "alice", p(1, 1)), enter(2, "bob", p(1, 1))]);
    let (a, b) = (
        r.zone.position_of(PlayerId::new(1)),
        r.zone.position_of(PlayerId::new(2)),
    );
    assert_eq!(a, Some(p(1, 1)));
    assert_ne!(a, b);
    // Walking onto someone stops beside them.
    r.tick(&[enter(3, "carol", p(4, 1)), move_to(3, p(1, 1))]);
    for _ in 0..30 {
        r.tick(&[]);
    }
    let c = r.zone.position_of(PlayerId::new(3)).unwrap();
    assert_ne!(Some(c), a);
    assert_ne!(Some(c), b);
    assert_eq!(c.distance(p(1, 1)), 1);
}

#[test]
fn walker_goes_around_someone_standing() {
    let mut r = Run::new();
    // Alice stands in the middle of a straight line from (0,3) to (4,3).
    r.tick(&[
        enter(1, "alice", p(2, 3)),
        enter(2, "bob", p(0, 3)),
        move_to(2, p(4, 3)),
    ]);
    for _ in 0..40 {
        r.tick(&[]);
    }
    assert_eq!(r.zone.position_of(PlayerId::new(2)), Some(p(4, 3)));
    assert_eq!(r.zone.position_of(PlayerId::new(1)), Some(p(2, 3)));
}

#[test]
fn blocked_corridor_gives_up() {
    let mut r = Run::new();
    // The only gap in the wall is (6, 6); alice stands in it.
    r.tick(&[
        enter(1, "alice", p(6, 6)),
        enter(2, "bob", p(4, 6)),
        move_to(2, p(8, 6)),
    ]);
    for _ in 0..(BLOCKED_GIVE_UP_TICKS + 10) {
        r.tick(&[]);
    }
    let b = r.zone.position_of(PlayerId::new(2)).unwrap();
    assert_eq!(b, p(5, 6), "waits at the gap");
    // Gave up: after alice leaves, bob does not walk on by himself.
    r.tick(&[Command::Leave {
        player: PlayerId::new(1),
    }]);
    for _ in 0..20 {
        r.tick(&[]);
    }
    assert_eq!(r.zone.position_of(PlayerId::new(2)), Some(b));
}

fn scenario() -> Vec<Vec<Command>> {
    let mut ticks: Vec<Vec<Command>> = vec![Vec::new(); 200];
    ticks[0] = vec![
        enter(1, "alice", p(0, 0)),
        enter(2, "bob", p(9, 0)),
        enter(3, "carol", p(3, 5)),
    ];
    ticks[5] = vec![Command::Move {
        player: PlayerId::new(2),
        destination: p(0, 6),
    }];
    ticks[9] = vec![Command::Move {
        player: PlayerId::new(1),
        destination: p(11, 6),
    }];
    ticks[60] = vec![
        Command::Stop {
            player: PlayerId::new(1),
        },
        Command::Leave {
            player: PlayerId::new(3),
        },
    ];
    ticks[70] = vec![enter(4, "dave", p(10, 1))];
    ticks
}

#[test]
fn deterministic_and_replayable() {
    let run = || {
        let mut r = Run::new();
        scenario()
            .iter()
            .map(|c| {
                r.tick(c);
                r.zone.state_hash()
            })
            .collect::<Vec<_>>()
    };
    let a = run();
    assert_eq!(a, run());

    // Replay from the log's base reproduces every hash.
    let mut r = Run::new();
    for c in scenario() {
        r.tick(&c);
    }
    let bytes = r.zone.replay_log().to_bytes().unwrap();
    let log = ReplayLog::from_bytes(&bytes).unwrap();
    let base = log.base().unwrap();
    let mut z = Zone::from_snapshot(catalog(), ZoneId::new(0), log.seed, base).unwrap();
    let mut out = TickOutput::default();
    for rec in log.records() {
        z.tick(
            &TickInput {
                commands: &rec.commands,
                now_unix: rec.now_unix,
                load: rec.load,
            },
            &mut out,
        );
        assert_eq!(z.state_hash(), rec.state_hash, "tick {}", rec.tick.0);
    }
    assert_eq!(log.len(), 200);
}

/// Same map with an elder at (2, 2) whose dialogue has two nodes.
fn catalog_with_npc() -> Arc<Content> {
    content_from(npc_files())
}

fn npc_files() -> Vec<(String, String)> {
    let text = r#"{
        "npc.elder.name": "Elder",
        "npc.elder.greeting": "Welcome.",
        "npc.elder.about": "I watch the road.",
        "npc.elder.option.about": "Who are you?",
        "npc.elder.option.bye": "Farewell.",
    }"#;
    let mut files: Vec<(String, String)> = [i18n::Locale::EN]
        .iter()
        .map(|l| (format!("text/{l}/a.ron"), text.to_owned()))
        .collect();
    files.push((
        "zones/test/zone.ron".into(),
        "(origin: (x: 100, y: 100), view_radius: 20, spawn: (x: 101, y: 101))".into(),
    ));
    files.push(("zones/test/map.ron".into(), MAP.into()));
    files.push((
        "zones/test/npcs.ron".into(),
        r#"[(npc: "elder", pos: (x: 102, y: 102))]"#.into(),
    ));
    files.push((
        "npcs/elder.ron".into(),
        r#"(sprite: "elder", dialogue: "elder")"#.into(),
    ));
    files.push((
        "npcs/dialogues/elder.ron".into(),
        r#"(
            start: "greeting",
            nodes: {
                "greeting": (
                    text: "npc.elder.greeting",
                    options: [
                        (text: "npc.elder.option.about", next: "about"),
                        (text: "npc.elder.option.bye"),
                    ],
                ),
                "about": (text: "npc.elder.about", options: [(text: "npc.elder.option.bye")]),
            },
        )"#
        .into(),
    ));
    files
}

fn npc_run() -> (Run, base::EntityId) {
    let mut run = Run {
        zone: Zone::new(catalog_with_npc(), ZoneId::new(0), 42).unwrap(),
        out: TickOutput::default(),
    };
    run.tick(&[enter(1, "Ana", p(4, 6))]);
    let npc = run
        .out
        .entities
        .iter()
        .find(|e| e.kind == EntityKind::Npc)
        .map(|e| e.id)
        .expect("the elder is visible");
    (run, npc)
}

fn talk(target: base::EntityId) -> Command {
    Command::Talk {
        player: PlayerId::new(1),
        target,
    }
}

fn choose(option: u16) -> Command {
    Command::ChooseOption {
        player: PlayerId::new(1),
        option,
    }
}

fn only_dialogue(out: &TickOutput) -> DialogueOut {
    assert_eq!(out.dialogue.len(), 1, "{:?}", out.dialogue);
    out.dialogue[0].clone()
}

#[test]
fn npc_is_seen_with_its_name_key() {
    let (run, npc) = npc_run();
    let state = run.out.entities.iter().find(|e| e.id == npc).unwrap();
    assert_eq!(state.pos, p(2, 2));
    // The only sprite in this content: position 0, sent as 1.
    assert_eq!(state.appearance, 1);
    assert!(matches!(state.label, EntityLabel::Npc(_)));
}

#[test]
fn dialogue_needs_reach_and_follows_options() {
    let (mut run, npc) = npc_run();
    let me = PlayerId::new(1);
    let out = run.tick(&[talk(npc)]);
    assert_eq!(
        only_dialogue(out),
        DialogueOut::Refused {
            player: me,
            reason: Refusal::TooFar
        }
    );
    let out = run.tick(&[choose(0)]);
    assert!(matches!(
        only_dialogue(out),
        DialogueOut::Refused {
            reason: Refusal::NoDialogue,
            ..
        }
    ));
    // Walk next to the elder.
    run.tick(&[move_to(1, p(3, 3))]);
    for _ in 0..80 {
        run.tick(&[]);
    }
    let out = run.tick(&[talk(npc)]);
    let DialogueOut::Show { text, options, .. } = only_dialogue(out) else {
        panic!("expected the greeting")
    };
    assert_eq!(options.len(), 2);
    let out = run.tick(&[choose(0)]);
    let DialogueOut::Show {
        text: about, options, ..
    } = only_dialogue(out)
    else {
        panic!("expected the next node")
    };
    assert_ne!(text, about);
    assert_eq!(options.len(), 1);
    let out = run.tick(&[choose(5)]);
    assert!(matches!(only_dialogue(out), DialogueOut::Refused { .. }));
    let out = run.tick(&[choose(0)]);
    assert_eq!(only_dialogue(out), DialogueOut::End { player: me, npc });
    let out = run.tick(&[choose(0)]);
    assert!(matches!(only_dialogue(out), DialogueOut::Refused { .. }));
}

#[test]
fn walking_away_closes_the_dialogue() {
    let (mut run, npc) = npc_run();
    run.tick(&[move_to(1, p(3, 3))]);
    for _ in 0..80 {
        run.tick(&[]);
    }
    assert!(matches!(
        only_dialogue(run.tick(&[talk(npc)])),
        DialogueOut::Show { .. }
    ));
    run.tick(&[move_to(1, p(5, 0))]);
    let mut closed = false;
    for _ in 0..80 {
        if let Some(d) = run.tick(&[]).dialogue.first() {
            assert_eq!(
                *d,
                DialogueOut::End {
                    player: PlayerId::new(1),
                    npc
                }
            );
            closed = true;
        }
    }
    assert!(closed, "the dialogue closes once out of reach");
    // Once.
    assert!(run.tick(&[]).dialogue.is_empty());
}

/// The elder gives a quest: talk to him, hold 2 herbs; a gift tops the
/// player up to 2 herbs (`TopUpItem`); completing pays 7 coins and a herb pile.
fn quest_files() -> Vec<(String, String)> {
    let mut files = npc_files();
    for (path, text) in files.iter_mut() {
        if path.starts_with("text/") {
            *text = text.replace(
                "\"npc.elder.name\": \"Elder\",",
                "\"npc.elder.name\": \"Elder\", \"item.herb.name\": \"Herb\", \"item.herb.desc\": \"-\", \
                 \"item.rock.name\": \"Rock\", \"item.rock.desc\": \"-\", \
                 \"quest.herbs.name\": \"Herbs\", \"quest.herbs.desc\": \"-\",",
            );
        }
    }
    files.push(("items/herb.ron".into(), "(kind: Material, max_stack: 5)".into()));
    files.push(("items/rock.ron".into(), "(kind: Material)".into()));
    files.push((
        "quests/herbs.ron".into(),
        r#"(objectives: [Talk(npc: "elder"), Item(item: "herb", qty: 2)], rewards: [Coins(7), Item(item: "herb", qty: 1)])"#.into(),
    ));
    if let Some(d) = files.iter_mut().find(|(p, _)| p == "npcs/dialogues/elder.ron") {
        d.1 = r#"(
            start: "greeting",
            nodes: {
                "greeting": (
                    text: "npc.elder.greeting",
                    options: [
                        (text: "npc.elder.option.about", next: "greeting",
                         when: Quest(quest: "herbs", state: NotStarted), effects: [StartQuest("herbs")]),
                        (text: "npc.elder.option.about", next: "greeting",
                         when: Not(Item(item: "herb", qty: 2)), effects: [TopUpItem(item: "herb", qty: 2), Affinity(3)]),
                        (text: "npc.elder.option.bye", when: QuestReady("herbs"), effects: [CompleteQuest("herbs")]),
                        (text: "npc.elder.option.bye", next: "greeting",
                         effects: [GiveCoins(1), GiveItem(item: "rock", qty: 100)]),
                    ],
                ),
            },
        )"#
        .into();
    }
    files
}

fn quest_run() -> (Run, base::EntityId) {
    let mut run = Run {
        zone: Zone::new(content_from(quest_files()), ZoneId::new(0), 42).unwrap(),
        out: TickOutput::default(),
    };
    run.tick(&[enter(1, "Ana", p(3, 3))]);
    let npc = run
        .out
        .entities
        .iter()
        .find(|e| e.kind == EntityKind::Npc)
        .map(|e| e.id)
        .unwrap();
    (run, npc)
}

fn shown(out: &TickOutput) -> usize {
    match out.dialogue.last() {
        Some(DialogueOut::Show { options, .. }) => options.len(),
        other => panic!("expected a node, got {other:?}"),
    }
}

#[test]
fn quest_with_conditions_and_effects() {
    let (mut run, npc) = quest_run();
    let me = PlayerId::new(1);
    // Entering sends the empty backpack once.
    assert_eq!(run.out.inventories.len(), 1);
    assert_eq!(shown(run.tick(&[talk(npc)])), 3); // start, gift, rocks
    // Start the quest: the talk objective counts on the next talk.
    let out = run.tick(&[choose(0)]);
    assert_eq!(shown(out), 2);
    assert_eq!(out.quest_logs.len(), 1);
    let out = run.tick(&[talk(npc)]);
    let log = &out.quest_logs[0].1;
    assert!(matches!(
        log[0].objectives[0],
        ObjectiveView::Talk { done: true, .. }
    ));
    assert_eq!(
        log[0].objectives[1],
        ObjectiveView::Item {
            item: base::ItemId::new(0),
            have: 0,
            need: 2
        }
    );
    // The gift tops up to 2 herbs: now the quest is ready and it is hidden.
    let out = run.tick(&[choose(0)]);
    assert_eq!(out.inventories[0].1.slots[0].map(|s| s.1), Some(2));
    assert_eq!(shown(out), 2); // complete, rocks
    let out = run.tick(&[choose(0)]);
    assert_eq!(out.dialogue[0], DialogueOut::End { player: me, npc });
    assert_eq!(out.inventories[0].1.coins, 7);
    assert_eq!(out.inventories[0].1.slots[0].map(|s| s.1), Some(1));
    assert!(out.quest_logs[0].1[0].completed);
    // A completed quest does not follow the backpack: holding 1, the gift
    // gives only the missing herb, no new log.
    assert_eq!(shown(run.tick(&[talk(npc)])), 2); // gift, rocks
    let out = run.tick(&[choose(0)]);
    assert_eq!(out.inventories[0].1.slots[0].map(|s| s.1), Some(2));
    assert!(out.quest_logs.is_empty());
    // Saved by text id, affinity included.
    run.tick(&[Command::Leave { player: me }]);
    let (_, sheet, _) = &run.out.left[0];
    assert_eq!(sheet.coins, 7);
    assert_eq!(
        sheet.backpack[0],
        Some(SavedStack {
            item: "herb".into(),
            qty: 2
        })
    );
    assert_eq!(sheet.quests[0].quest, "herbs");
    assert!(sheet.quests[0].completed);
    assert_eq!(sheet.affinity, vec![("elder".to_owned(), 6)]);
}

#[test]
fn effects_are_all_or_nothing() {
    let (mut run, npc) = quest_run();
    run.tick(&[talk(npc)]);
    // GiveCoins then more rocks than the backpack holds: nothing changes.
    let out = run.tick(&[choose(2)]);
    assert_eq!(
        out.dialogue[0],
        DialogueOut::Refused {
            player: PlayerId::new(1),
            reason: Refusal::InventoryFull
        }
    );
    assert!(out.inventories.is_empty());
}

#[test]
fn move_item_merges_and_swaps() {
    let (mut run, npc) = quest_run();
    run.tick(&[talk(npc)]);
    run.tick(&[choose(1)]); // 2 herbs in slot 0
    let mv = |from, to| Command::MoveItem {
        player: PlayerId::new(1),
        from,
        to,
    };
    let out = run.tick(&[mv(0, 7)]);
    assert_eq!(out.inventories[0].1.slots[7].map(|s| s.1), Some(2));
    let out = run.tick(&[mv(3, 4)]);
    assert_eq!(out.refused, vec![(PlayerId::new(1), Refusal::InvalidSlot)]);
    let out = run.tick(&[mv(7, 99)]);
    assert_eq!(out.refused.len(), 1);
}

#[test]
fn unknown_saved_ids_are_kept() {
    let mut run = Run {
        zone: Zone::new(content_from(quest_files()), ZoneId::new(0), 42).unwrap(),
        out: TickOutput::default(),
    };
    let mut sheet = CharacterSheet::new(CharacterName::try_from("Ana").unwrap());
    sheet.backpack = vec![
        None,
        Some(SavedStack {
            item: "gone".into(),
            qty: 4,
        }),
    ];
    sheet.quests = vec![SavedQuest {
        quest: "old".into(),
        completed: true,
        talked: 0,
        completed_at: 0,
        progress: Vec::new(),
    }];
    sheet.affinity = vec![("ghost".into(), -5)];
    let me = PlayerId::new(1);
    run.tick(&[Command::Enter {
        player: me,
        sheet: Box::new(sheet.clone()),
        pos: Some(p(3, 3)),
    }]);
    // The unknown slot is not usable.
    let out = run.tick(&[Command::MoveItem {
        player: me,
        from: 1,
        to: 2,
    }]);
    assert_eq!(out.refused.len(), 1);
    run.tick(&[Command::Leave { player: me }]);
    let (_, back, _) = &run.out.left[0];
    assert_eq!(back.backpack[1], sheet.backpack[1]);
    assert_eq!(back.quests, sheet.quests);
    assert_eq!(back.affinity, sheet.affinity);
}

#[test]
fn reload_keeps_players_and_respawns_npcs() {
    let (mut run, npc) = quest_run();
    run.tick(&[talk(npc)]);
    run.tick(&[choose(1)]); // 2 herbs
    // New content: one more item sorts before `herb` and shifts its id.
    let mut files = quest_files();
    for (path, text) in files.iter_mut() {
        if path.starts_with("text/") {
            *text = text.replace(
                "\"item.herb.name\"",
                "\"item.apple.name\": \"A\", \"item.apple.desc\": \"-\", \"item.herb.name\"",
            );
        }
    }
    files.push(("items/apple.ron".into(), "(kind: Consumable)".into()));
    let new = content_from(files);
    let mut out = TickOutput::default();
    run.zone.reload(new.clone(), &mut out).unwrap();
    assert!(out.reloaded);
    assert_eq!(
        out.dialogue,
        vec![DialogueOut::End {
            player: PlayerId::new(1),
            npc
        }]
    );
    let out = run.tick(&[]);
    let herb = new.catalog.item_by_name("herb").unwrap().0;
    assert_eq!(out.inventories[0].1.slots[0], Some((herb, 2)));
    // The old NPC left the view and a new one entered.
    let view = &out.views[0].1;
    assert_eq!(view.left, vec![npc]);
    assert_eq!(view.entered.len(), 1);
    // Content without `rock` is refused.
    let mut files = quest_files();
    files.retain(|(p, _)| p != "items/rock.ron");
    if let Some(d) = files.iter_mut().find(|(p, _)| p == "npcs/dialogues/elder.ron") {
        d.1 = d.1.replace(", GiveItem(item: \"rock\", qty: 100)", "");
    }
    let mut out = TickOutput::default();
    assert!(matches!(
        run.zone.reload(content_from(files), &mut out),
        Err(ZoneError::Removed(_))
    ));
}

#[test]
fn markers_follow_the_quest() {
    let (mut run, npc) = quest_run();
    let marks = |out: &TickOutput| out.markers.last().map(|(_, m)| m.clone());
    assert_eq!(marks(&run.out), Some(vec![(npc, QuestMarker::Available)]));
    run.tick(&[talk(npc)]);
    let out = run.tick(&[choose(0)]); // start: talk to the elder
    assert_eq!(marks(out), Some(vec![(npc, QuestMarker::Talk)]));
    let out = run.tick(&[talk(npc)]); // talked; herbs missing
    assert_eq!(marks(out), Some(vec![]));
    let out = run.tick(&[choose(0)]); // the gift: 2 herbs
    assert_eq!(marks(out), Some(vec![(npc, QuestMarker::Ready)]));
}

#[test]
fn close_dialogue_ends_it() {
    let (mut run, npc) = quest_run();
    let me = PlayerId::new(1);
    run.tick(&[talk(npc)]);
    let out = run.tick(&[Command::CloseDialogue { player: me }]);
    assert_eq!(out.dialogue, vec![DialogueOut::End { player: me, npc }]);
    // Nothing open: nothing to close, and options are refused.
    let out = run.tick(&[Command::CloseDialogue { player: me }]);
    assert!(out.dialogue.is_empty());
    let out = run.tick(&[choose(0)]);
    assert!(matches!(out.dialogue[0], DialogueOut::Refused { .. }));
}

/// Phase 6c: a repainted map of the same size applies on reload; a resized
/// one waits for a restart.
#[test]
fn reload_applies_a_repainted_map_of_the_same_size() {
    let (mut run, _) = quest_run();
    let o = run.zone.map().origin();
    let wall = TilePos::new(o.x + 6, o.y);
    assert!(run.zone.map().blocks_movement(wall));
    let repaint = |from: &str, to: &str| {
        let mut files = quest_files();
        for (path, text) in files.iter_mut() {
            if path == "zones/test/map.ron" {
                *text = text.replacen(from, to, 1);
            }
        }
        content_from(files)
    };
    let mut out = TickOutput::default();
    run.zone
        .reload(repaint("\"......#.....\"", "\"............\""), &mut out)
        .unwrap();
    assert!(out.map_changed);
    assert!(!run.zone.map().blocks_movement(wall));
    let mut out = TickOutput::default();
    run.zone
        .reload(
            repaint("\"......#.....\",", "\"......#.....\", \"............\","),
            &mut out,
        )
        .unwrap();
    assert!(!out.map_changed, "a resized map waits for a restart");
    assert!(!run.zone.map().blocks_movement(wall));
}

/// The world and the two indexes agree through every way an entity comes
/// and goes (`zone::entities`). `Zone::tick` checks the same invariant in
/// every debug build; this test names the paths that could break it.
#[test]
fn the_indexes_follow_the_world() {
    let mut r = Run::new();
    let check = |z: &Zone, what: &str| {
        assert_eq!(z.index_error(), None, "after {what}");
    };
    check(&r.zone, "the zone was built");
    r.tick(&[enter(1, "Ana", p(5, 5)), enter(2, "Bia", p(6, 5))]);
    check(&r.zone, "two players entered");
    assert_eq!(r.zone.player_count(), 2);

    r.tick(&[move_to(1, p(9, 9))]);
    check(&r.zone, "a player walked");

    // A reload throws away every NPC and monster and places them again.
    let mut out = TickOutput::default();
    r.zone.reload(catalog(), &mut out).unwrap();
    check(&r.zone, "the content was reloaded");

    r.tick(&[Command::Leave {
        player: PlayerId::new(1),
    }]);
    check(&r.zone, "a player left");
    assert_eq!(r.zone.player_count(), 1);

    // Leaving twice must not take the other player out with it.
    r.tick(&[Command::Leave {
        player: PlayerId::new(1),
    }]);
    check(&r.zone, "the same player left twice");
    assert_eq!(r.zone.player_count(), 1);

    // A zone rebuilt from a snapshot indexes what the snapshot holds.
    let state = r.zone.snapshot();
    let back = Zone::from_snapshot(catalog(), ZoneId::new(0), 42, &state).unwrap();
    check(&back, "a snapshot was restored");
    assert_eq!(back.state_hash(), r.zone.state_hash());
}

/// The elder teaches one recipe for free, only to who does not know it.
fn lesson_files() -> Vec<(String, String)> {
    let mut files = npc_files();
    for (path, text) in files.iter_mut() {
        if path.starts_with("text/") {
            *text = r#"{
                "npc.elder.name": "Elder",
                "npc.elder.greeting": "Welcome.",
                "npc.elder.about": "I watch the road.",
                "npc.elder.option.about": "Teach me.",
                "npc.elder.option.bye": "Farewell.",
                "proficiency.smithing.name": "Smithing",
                "proficiency.unarmed.name": "Unarmed",
                "recipe.blade.name": "Blade",
                "recipe.blade.desc": "A blade.",
                "item.ingot.name": "Ingot",
                "item.ingot.desc": "Metal.",
                "item.knife.name": "Knife",
                "item.knife.desc": "Sharp.",
            }"#
            .to_owned();
        }
    }
    files.push(("proficiencies/unarmed.ron".into(), "(kind: Weapon)".into()));
    files.push(("proficiencies/smithing.ron".into(), "(kind: Production)".into()));
    files.push(("items/ingot.ron".into(), "(kind: Material, max_stack: 20)".into()));
    files.push(("items/knife.ron".into(), "(kind: Material)".into()));
    files.push((
        "recipes/blade.ron".into(),
        r#"(proficiency: "smithing", materials: [(item: "ingot", qty: 1)], result: "knife", xp: 1)"#.into(),
    ));
    if let Some(d) = files.iter_mut().find(|(p, _)| p == "npcs/dialogues/elder.ron") {
        d.1 = r#"(
            start: "greeting",
            nodes: {
                "greeting": (
                    text: "npc.elder.greeting",
                    options: [
                        (text: "npc.elder.option.about", next: "greeting",
                         when: Not(RecipeKnown("blade")), effects: [LearnRecipe("blade")]),
                        (text: "npc.elder.option.bye"),
                    ],
                ),
            },
        )"#
        .into();
    }
    files
}

#[test]
fn a_lesson_is_not_offered_again_once_learned() {
    let mut run = Run {
        zone: Zone::new(content_from(lesson_files()), ZoneId::new(0), 42).unwrap(),
        out: TickOutput::default(),
    };
    run.tick(&[enter(1, "Ana", p(3, 3))]);
    let npc = run
        .out
        .entities
        .iter()
        .find(|e| e.kind == EntityKind::Npc)
        .map(|e| e.id)
        .unwrap();
    assert_eq!(shown(run.tick(&[talk(npc)])), 2, "the lesson and the farewell");
    // Learning it: back to the greeting, now with the farewell only, and
    // the sheet (with the recipe) goes out in the same tick.
    let out = run.tick(&[choose(0)]);
    assert_eq!(shown(out), 1);
    let sent = &run.out.stats;
    assert!(
        sent.iter().any(|(_, s)| !s.recipes.is_empty()),
        "the new recipe reaches the client at once"
    );
    run.tick(&[choose(0)]);
    assert_eq!(
        shown(run.tick(&[talk(npc)])),
        1,
        "still known after talking again"
    );
}

/// The quest log says where to go next: the elder to talk to,
/// and, with nothing in the world that drops herbs, the elder again.
#[test]
fn the_quest_log_points_where_to_go() {
    let (mut run, npc) = quest_run();
    run.tick(&[talk(npc)]);
    let out = run.tick(&[choose(0)]);
    let log = &out.quest_logs[0].1;
    assert_eq!(log[0].goal, Some(p(2, 2)), "talk to the elder");
    let out = run.tick(&[talk(npc)]);
    assert_eq!(
        out.quest_logs[0].1[0].goal,
        Some(p(2, 2)),
        "the elder takes it in"
    );
}

/// A quest reward may teach a technique: completing it through the
/// dialogue puts the technique in the sheet at level 1.
#[test]
fn a_quest_reward_teaches_a_technique() {
    let mut files = quest_files();
    for (path, text) in files.iter_mut() {
        if path.starts_with("text/") {
            *text = text.replace(
                "\"npc.elder.name\": \"Elder\",",
                "\"npc.elder.name\": \"Elder\", \"proficiency.light.name\": \"Light\", \"proficiency.unarmed.name\": \"Unarmed\", \
                 \"technique.vow.name\": \"Vow\", \"technique.vow.desc\": \"-\",",
            );
        }
        if path == "quests/herbs.ron" {
            *text = text.replace("rewards: [", "rewards: [Technique(\"vow\"), ");
        }
    }
    files.push(("proficiencies/light.ron".into(), "(kind: Magic)".into()));
    files.push(("proficiencies/unarmed.ron".into(), "(kind: Weapon)".into()));
    files.push((
        "techniques/vow.ron".into(),
        "(proficiency: \"light\", unlock_at: 1, cooldown_ticks: 200, vigor: 5, \
         buff: Some((stat: Defense, permille: 500, flat: 4, duration_ticks: 100)), taught_only: true)"
            .into(),
    ));
    let content = content_from(files);
    let vow = content
        .catalog
        .techniques()
        .find(|(_, t)| t.id == "vow")
        .map(|(id, _)| id)
        .unwrap();
    let mut run = Run {
        zone: Zone::new(content, ZoneId::new(0), 42).unwrap(),
        out: TickOutput::default(),
    };
    run.tick(&[enter(1, "Ana", p(3, 3))]);
    let npc = run
        .out
        .entities
        .iter()
        .find(|e| e.kind == EntityKind::Npc)
        .map(|e| e.id)
        .unwrap();
    let knows = |run: &Run| {
        let h = run.zone.by_player[&PlayerId::new(1)];
        let p = run.zone.world.get::<&crate::zone::PlayerComp>(h).unwrap();
        p.growth.sheet.technique(vow).is_some()
    };
    run.tick(&[talk(npc)]);
    run.tick(&[choose(0)]); // start
    run.tick(&[talk(npc)]); // talk objective
    run.tick(&[choose(0)]); // gift: 2 herbs
    assert!(!knows(&run));
    run.tick(&[choose(0)]); // complete
    assert!(knows(&run), "the reward teaches the technique");
}

/// A brazier at (3, 0) of a small zone; the player starts at (1, 1).
fn brazier_run() -> Run {
    let text = r#"{ "system.ok": "x" }"#;
    let mut files: Vec<(String, String)> = [i18n::Locale::EN]
        .iter()
        .map(|l| (format!("text/{l}/a.ron"), text.to_owned()))
        .collect();
    files.push((
        "zones/test/zone.ron".into(),
        "(origin: (x: 100, y: 100), view_radius: 20, spawn: (x: 101, y: 101))".into(),
    ));
    let map = r#"(
        legend: { '.': (floor: 1), '*': (floor: 6, blocks_movement: true, switch: Some(19)) },
        rows: ["...*........", "............", "............", "............", "............", "............", "............"],
    )"#;
    files.push(("zones/test/map.ron".into(), map.into()));
    let mut r = Run {
        zone: Zone::new(content_from(files), ZoneId::new(0), 42).unwrap(),
        out: TickOutput::default(),
    };
    r.tick(&[enter(1, "Ana", p(1, 1))]);
    r
}

fn use_tile(at: TilePos) -> Command {
    Command::UseTile {
        player: PlayerId::new(1),
        pos: at,
    }
}

#[test]
fn a_player_lights_and_puts_out_a_brazier() {
    let mut r = brazier_run();
    let out = r.tick(&[use_tile(p(3, 0))]);
    let word = 19 | world::TILE_BLOCKS | world::TILE_SWITCH;
    assert_eq!(out.tiles, vec![(p(3, 0), word)], "put out, still blocking");
    assert!(out.refused.is_empty());
    let out = r.tick(&[use_tile(p(3, 0))]);
    assert_eq!(out.tiles[0].1 & world::TILE_FLOOR, 6, "and lit again");
    let out = r.tick(&[use_tile(p(5, 5))]);
    assert_eq!(out.refused, vec![(PlayerId::new(1), Refusal::NothingToUse)]);
    assert!(out.tiles.is_empty(), "grass does not switch");
    let out = r.tick(&[use_tile(p(100, 100))]);
    assert_eq!(
        out.refused,
        vec![(PlayerId::new(1), Refusal::NothingToUse)],
        "outside the zone"
    );
}

#[test]
fn a_brazier_out_of_reach_is_refused() {
    let mut r = brazier_run();
    r.tick(&[move_to(1, p(11, 6))]);
    for _ in 0..60 {
        r.tick(&[]);
    }
    let out = r.tick(&[use_tile(p(3, 0))]);
    assert_eq!(out.refused, vec![(PlayerId::new(1), Refusal::TooFar)]);
    assert!(out.tiles.is_empty());
}

/// Karma: a dialogue effect moves it, a condition reads it, a title is
/// earned by it at once, and it is saved.
#[test]
fn karma_moves_opens_and_earns_a_title() {
    let mut files = quest_files();
    for (path, text) in &mut files {
        if path.starts_with("text/") {
            *text = text.replace(
                "\"npc.elder.name\": \"Elder\",",
                "\"npc.elder.name\": \"Elder\", \"title.merciful.name\": \"Merciful\", \
                 \"title.merciful.desc\": \"-\",",
            );
        }
    }
    files.push(("titles/merciful.ron".into(), "(trigger: Karma(10))".into()));
    if let Some(d) = files.iter_mut().find(|(p, _)| p == "npcs/dialogues/elder.ron") {
        d.1 = r#"(
            start: "greeting",
            nodes: {
                "greeting": (
                    text: "npc.elder.greeting",
                    options: [
                        (text: "npc.elder.option.about", next: "greeting",
                         when: Not(Karma(10)), effects: [Karma(15)]),
                        (text: "npc.elder.option.bye", when: Karma(10)),
                    ],
                ),
            },
        )"#
        .into();
    }
    let mut run = Run {
        zone: Zone::new(content_from(files), ZoneId::new(0), 42).unwrap(),
        out: TickOutput::default(),
    };
    run.tick(&[enter(1, "Ana", p(3, 3))]);
    let npc = run
        .out
        .entities
        .iter()
        .find(|e| e.kind == EntityKind::Npc)
        .map(|e| e.id)
        .unwrap();
    assert_eq!(shown(run.tick(&[talk(npc)])), 1, "only the kind deed");
    assert_eq!(shown(run.tick(&[choose(0)])), 1, "now only the karma door");
    run.tick(&[Command::Leave {
        player: PlayerId::new(1),
    }]);
    let (_, sheet, _) = &run.out.left[0];
    assert_eq!(sheet.progress.karma, 15);
    assert!(
        sheet.progress.titles.contains(&"merciful".to_owned()),
        "earned on the answer"
    );
}

/// Adventurer rank: a quest above the character's rank cannot start; a
/// completed quest of the rank raises it. A barber changes the hair.
#[test]
fn rank_gates_quests_and_a_barber_cuts_hair() {
    let mut files = npc_files();
    for (path, text) in &mut files {
        if path.starts_with("text/") {
            *text = text.replace(
                "\"npc.elder.name\": \"Elder\",",
                "\"npc.elder.name\": \"Elder\", \"quest.chat.name\": \"-\", \"quest.chat.desc\": \"-\", \
                 \"quest.hunt.name\": \"-\", \"quest.hunt.desc\": \"-\", \
                 \"appearance.body.b\": \"-\", \"appearance.skin.s\": \"-\", \
                 \"appearance.hair_style.short\": \"-\", \"appearance.hair_style.long\": \"-\", \
                 \"appearance.hair_color.black\": \"-\", \"system.rank_up\": \"{rank}\",",
            );
        }
    }
    files.push((
        "appearance/appearance.ron".into(),
        r#"(bodies: [(id: "b")], skins: [(id: "s")], hair_styles: [(id: "short"), (id: "long")], hair_colors: [(id: "black")])"#.into(),
    ));
    files.push(("progression/formulas.ron".into(), "(quests_per_rank: 1)".into()));
    files.push((
        "quests/chat.ron".into(),
        r#"(objectives: [Talk(npc: "elder")])"#.into(),
    ));
    files.push((
        "quests/hunt.ron".into(),
        r#"(rank: E, objectives: [Talk(npc: "elder")])"#.into(),
    ));
    if let Some(d) = files.iter_mut().find(|(p, _)| p == "npcs/dialogues/elder.ron") {
        d.1 = r#"(
            start: "greeting",
            nodes: {
                "greeting": (
                    text: "npc.elder.greeting",
                    options: [
                        (text: "npc.elder.option.about", next: "greeting",
                         when: Quest(quest: "chat", state: NotStarted), effects: [StartQuest("hunt")]),
                        (text: "npc.elder.option.about", next: "greeting",
                         when: Quest(quest: "chat", state: NotStarted), effects: [StartQuest("chat")]),
                        (text: "npc.elder.option.bye", next: "greeting",
                         when: QuestReady("chat"), effects: [CompleteQuest("chat")]),
                        (text: "npc.elder.option.bye", next: "greeting",
                         when: Quest(quest: "chat", state: Completed),
                         effects: [StartQuest("hunt"), SetHairStyle("long")]),
                    ],
                ),
            },
        )"#
        .into();
    }
    let mut run = Run {
        zone: Zone::new(content_from(files), ZoneId::new(0), 42).unwrap(),
        out: TickOutput::default(),
    };
    run.tick(&[enter(1, "Ana", p(3, 3))]);
    let npc = run
        .out
        .entities
        .iter()
        .find(|e| e.kind == EntityKind::Npc)
        .map(|e| e.id)
        .unwrap();
    let me = PlayerId::new(1);
    assert_eq!(shown(run.tick(&[talk(npc)])), 2);
    // Rank E is above a new character's F.
    let out = run.tick(&[choose(0)]);
    assert!(
        matches!(out.dialogue.last(), Some(DialogueOut::Refused { .. })),
        "{:?}",
        out.dialogue
    );
    run.tick(&[choose(1)]);
    assert_eq!(shown(run.tick(&[talk(npc)])), 1, "chat is ready");
    let out = run.tick(&[choose(0)]);
    assert!(
        out.system
            .iter()
            .any(|(p, t)| *p == me && t.args.iter().any(|a| *a == base::Arg::Str("E".into()))),
        "rank up told"
    );
    let out = run.tick(&[choose(0)]);
    assert!(
        matches!(out.dialogue.last(), Some(DialogueOut::Show { .. })),
        "hunt starts at rank E"
    );
    run.tick(&[Command::Leave { player: me }]);
    let (_, sheet, _) = &run.out.left[0];
    assert!(sheet.quests.iter().any(|q| q.quest == "hunt"));
    assert_eq!(sheet.look[2], "long", "the barber's cut is saved");
}
