use super::*;
use base::Locale;

const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/riftara/assets");
const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/riftara");

/// Reads the example game's interface texts (once per test run).
fn init_game() {
    let game = std::path::Path::new(GAME);
    // Already set by another test: the same game, so the error is ignored.
    let _ = set_game(&game.join("content"), &game.join("assets"));
}

fn read(path: &str) -> Option<Vec<u8>> {
    std::fs::read(format!("{ASSETS}/{path}")).ok()
}

fn theme(id: &str) -> UiTheme {
    load_theme(&std::fs::read_to_string(format!("{ASSETS}/themes/{id}.ron")).unwrap()).unwrap()
}

pub(crate) fn ui(locale: Locale) -> Ui {
    init_game();
    let t = theme("isekai");
    let (fonts, problems) = Fonts::load(&t, locale.code(), &mut read);
    // The CJK font is downloaded apart (fetch-cjk.sh); Latin ones are in git.
    assert!(problems.iter().all(|p| p.contains("NotoSansCJK")), "{problems:?}");
    Ui::new(t, fonts, UiText::new(locale))
}

#[test]
fn themes_in_assets_load() {
    for id in ["isekai", "ember"] {
        let t = theme(id);
        assert_eq!(t.id, id);
    }
    assert_eq!(parse_color("#00c8ff"), Some([0.0, 200.0 / 255.0, 1.0, 1.0]));
    assert_eq!(parse_color("00c8ff"), None);
    assert!(load_theme("Theme(id: \"x\")").is_err());
}

#[test]
fn the_game_theme_names_a_theme_that_loads() {
    let text = std::fs::read_to_string(format!("{ASSETS}/{GAME_THEME_FILE}")).unwrap();
    let id = read_game_theme(&text).unwrap();
    assert_eq!(theme(&id).id, id);
    assert!(read_game_theme("GameTheme(theme: \"../x\")").is_err());
    assert!(read_game_theme("GameTheme(theme: \"\")").is_err());
}

#[test]
fn theme_rules_refuse_typos_and_bad_numbers() {
    let text = std::fs::read_to_string(format!("{ASSETS}/themes/isekai.ron")).unwrap();
    let file: ThemeFile = ron::from_str(&text).unwrap();
    assert_eq!(check_theme(&file), Ok(()));
    // Every color the file names is one the theme knows.
    assert!(file.colors.keys().all(|n| COLORS.iter().any(|c| c.name == n)));
    let mut typo = file.clone();
    typo.colors.insert("hilight".into(), "#ffffff".into());
    assert!(check_theme(&typo).unwrap_err().0.contains("hilight"));
    let mut tiny = file.clone();
    tiny.sizes.text = 0.0;
    assert!(check_theme(&tiny).is_err());
    let mut slow = file;
    slow.windows.open_ms = 9000;
    assert!(check_theme(&slow).is_err());
    // A theme without the optional colors takes their fallbacks.
    let t = theme("ember");
    assert_eq!(t.colors.cooldown, [0.0, 0.0, 0.0, 140.0 / 255.0]);
    assert_eq!(t.colors.error, t.colors.life);
}

#[test]
fn ui_texts_exist_in_every_required_locale() {
    init_game();
    let files = text_files();
    let en: Vec<(&str, &str)> = files
        .iter()
        .filter(|f| f.code == "en")
        .flat_map(|f| [("ui.ron", f.ui.as_str()), ("error.ron", f.error.as_str())])
        .collect();
    let keys = i18n::KeyTable::from_source(&en).unwrap();
    assert!(!files.is_empty());
    for f in files {
        let loc: Locale = f.code.parse().unwrap();
        let problems: Vec<_> = i18n::check_locale(
            &keys,
            loc,
            &[("ui.ron", f.ui.as_str()), ("error.ron", f.error.as_str())],
        )
        .into_iter()
        // An optional language may lack texts: English shows.
        .filter(|p| f.required || p.kind != i18n::TextProblemKind::MissingKey)
        .collect();
        assert!(problems.is_empty(), "{loc:?}: {problems:?}");
        let t = UiText::new(loc);
        assert_ne!(t.t("ui.login.enter"), "ui.login.enter");
        assert!(t.t_args("ui.hud.level", &[base::Arg::Int(7)]).contains('7'));
    }
}

#[test]
fn login_lists_only_required_languages() {
    init_game();
    let names = crate::language_names();
    let required: Vec<&str> = text_files()
        .iter()
        .filter(|f| f.required)
        .map(|f| f.code.as_str())
        .collect();
    assert_eq!(names.iter().map(|(l, _)| l.code()).collect::<Vec<_>>(), required);
}

fn input() -> Input {
    Input {
        screen: (1280.0, 720.0),
        scale: 1.0,
        mouse: (620.0, 700.0),
        clicked: true,
        // A tap: pressed and let go in the same place.
        released: true,
        press_at: Some((620.0, 700.0)),
        ..Input::default()
    }
}

pub(crate) fn model() -> HudModel {
    HudModel {
        name: "Alfa".into(),
        level: 3,
        hp: (20, 40),
        vigor: (5, 10),
        xp: (50, 100),
        target: Some(TargetModel {
            name: "Wolf".into(),
            level: 2,
            health: 60,
        }),
        hotbar: vec![
            Some(HotbarSlot {
                name: "Double cut".into(),
                tooltip: vec!["Level 1".into(), "Cuts twice.".into()],
                cooldown: 0.5,
                cooldown_text: Some("3".into()),
                usable: false,
                icon: None,
                count: None,
            });
            10
        ],
        hotbar_keys: (1..=10).map(|i| (i % 10).to_string()).collect(),
        log: vec![LogLine {
            kind: LineKind::System,
            text: "Level up!".into(),
        }],
        system: vec!["Title earned".into()],
        coins: 12,
        bag: vec![None; 20],
        minimap: Minimap {
            side: 4,
            tiles: vec![1; 16],
            dots: vec![(0, 0, Dot::Me)],
        },
        fallen: true,
        ..HudModel::default()
    }
}

#[test]
fn hud_draws_and_reports_clicks() {
    let mut u = ui(Locale::EN);
    u.hud.inventory = true;
    u.hud.sheet = true;
    let m = model();
    let out = u.frame(&input(), |f, hud| hud.draw(f, &m));
    assert!(!u.vertices().is_empty());
    assert!(out.over_ui, "the click landed on the hotbar");
    assert!(
        out.actions.iter().any(|a| matches!(a, UiAction::Hotbar(_))),
        "{:?}",
        out.actions
    );
    assert!(u.take_atlas().is_some());
    assert!(u.take_atlas().is_none(), "the atlas goes once per change");
}

#[test]
fn chat_sends_on_enter() {
    let mut u = ui(Locale::from_static("pt-BR"));
    u.hud.chat_open = true;
    let m = model();
    let mut i = input();
    i.clicked = false;
    i.released = false;
    i.typed = "olá".into();
    u.frame(&i, |f, hud| hud.draw(f, &m));
    i.typed.clear();
    i.enter = true;
    let out = u.frame(&i, |f, hud| hud.draw(f, &m));
    assert_eq!(out.actions, vec![UiAction::SendChat("olá".into())]);
    assert!(!u.hud.chat_open);
}

#[test]
fn wrap_breaks_at_spaces_and_inside_cjk() {
    let mut u = ui(Locale::EN);
    let lines = u
        .fonts
        .wrap(Role::Text, 16.0, "one two three four five six", 60.0);
    assert!(lines.len() > 1);
    assert!(lines.iter().all(|l| !l.starts_with(' ')));
}

#[test]
fn key_map_defaults_and_overrides() {
    let k = KeyMap::default();
    let chord = |s: &str| Chord::parse(s).unwrap();
    assert_eq!(k.actions(&chord("KeyW")), &[Action::MoveUp]);
    assert_eq!(k.actions(&chord("Digit0")), &[Action::Hotbar(9)]);
    assert!(k.actions(&chord("Escape")).contains(&Action::Menu));
    let (k, problems) = KeyMap::load(
        r#"(wasd: false, keys: {"open_inventory": "KeyB", "hotbar_1": "Shift+Digit1", "fly": "KeyX"})"#,
    );
    assert_eq!(problems.len(), 1);
    assert!(k.actions(&chord("KeyW")).is_empty(), "WASD off");
    assert_eq!(k.actions(&chord("KeyB")), &[Action::OpenInventory]);
    assert_eq!(k.actions(&chord("Shift+Digit1")), &[Action::Hotbar(0)]);
    assert!(k.actions(&chord("KeyI")).is_empty());
}

fn quiet() -> Input {
    Input {
        mouse: (-1.0, -1.0),
        clicked: false,
        ..input()
    }
}

#[test]
fn login_screen_types_tabs_and_logs_in() {
    let mut u = ui(Locale::EN);
    let screen = Screen::Login(LoginModel {
        servers: vec!["play.example:7000".into()],
        ..LoginModel::default()
    });
    let mut i = quiet();
    i.typed = "kai".into();
    u.screen(&i, &screen);
    let mut i = quiet();
    i.tab = true;
    let out = u.screen(&i, &screen);
    assert!(out.typing);
    let mut i = quiet();
    i.typed = "secret12".into();
    u.screen(&i, &screen);
    let mut i = quiet();
    i.enter = true;
    let out = u.screen(&i, &screen);
    assert_eq!(
        out.actions,
        vec![UiAction::Login {
            server: String::new(),
            account: "kai".into(),
            password: "secret12".into(),
            remember: false,
        }]
    );
    assert!(!u.vertices().is_empty());
}

/// A login background replaces the circle: it covers the screen with
/// sheet coordinates cropped to the screen's shape, never stretched.
#[test]
fn login_background_covers_the_screen_cropped() {
    let mut u = ui(Locale::EN);
    let screen = Screen::Login(LoginModel::default());
    // A square picture on a wide screen: the top and bottom are cut.
    u.screens.backdrop = Some(([2.0, 0.0, 2.5, 0.5], 1.0));
    let i = quiet();
    u.screen(&i, &screen);
    let textured: Vec<_> = u.vertices().iter().filter(|v| v.uv[0] >= 2.0).collect();
    assert!(!textured.is_empty());
    let vs: Vec<f32> = textured.iter().map(|v| v.uv[1]).collect();
    let (lo, hi) = vs
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    let (w, h) = i.screen;
    let keep = 0.5 * (h / w);
    assert!((hi - lo - keep).abs() < 1e-4, "{lo}..{hi}, expected {keep}");
}

/// The language is a list: a click opens it with the ten languages, each
/// in its own name; a row picks it; a click outside only closes it.
#[test]
fn login_language_list_opens_and_picks() {
    let mut u = ui(Locale::EN);
    let screen = Screen::Login(LoginModel {
        servers: vec!["play.example:7000".into(), "test.example:7000".into()],
        ..LoginModel::default()
    });
    let click = |x: f32, y: f32| Input {
        mouse: (x, y),
        press_at: Some((x, y)),
        ..input()
    };
    // Find the language box: the click that opens the language list.
    let mut at = None;
    'find: for y in (0..720).step_by(8) {
        for x in (0..1280).step_by(8) {
            u.screens.open_list = None;
            let (x, y) = (x as f32, y as f32);
            let out = u.screen(&click(x, y), &screen);
            assert!(out.actions.is_empty(), "{:?}", out.actions);
            if u.screens.open_list == Some(30) {
                at = Some((x + 16.0, y));
                break 'find;
            }
        }
    }
    let (x, y) = at.expect("no language box");
    // Rows below it: each picks one language, in list order.
    let mut picked = Vec::new();
    for dy in (8..720).step_by(4) {
        u.screens.open_list = Some(30);
        let out = u.screen(&click(x, y + dy as f32), &screen);
        for a in out.actions {
            if let UiAction::SetLocale(l) = a
                && picked.last() != Some(&l)
            {
                picked.push(l);
            }
        }
    }
    assert_eq!(
        picked,
        crate::language_names()
            .iter()
            .map(|(l, _)| *l)
            .collect::<Vec<_>>()
    );
    // A click far away closes it without acting.
    u.screens.open_list = Some(30);
    let out = u.screen(&click(5.0, 5.0), &screen);
    assert!(out.actions.is_empty() && u.screens.open_list.is_none());
    // The server list sets the server.
    u.screens.open_list = Some(1);
    for dy in (0..720).step_by(4) {
        u.screens.open_list = Some(1);
        u.screen(&click(640.0, dy as f32), &screen);
        if u.screens.server == "test.example:7000" {
            break;
        }
    }
    assert_eq!(u.screens.server, "test.example:7000");
}

/// IME (CJK): the composition shows in the focused field, the committed
/// text goes in, and the caret tells the window where the candidates go.
#[test]
fn ime_composition_and_commit_in_a_field() {
    let mut u = ui(Locale::from_static("ja"));
    let screen = Screen::Create(CreateModel::default());
    let mut i = quiet();
    i.preedit = "にほん".into();
    let out = u.screen(&i, &screen);
    assert!(out.typing && out.ime_area.is_some());
    assert!(u.screens.name.is_empty(), "composition is not text yet");
    let mut i = quiet();
    i.typed = "日本".into();
    u.screen(&i, &screen);
    assert_eq!(u.screens.name, "日本");
    // Names are [A-Za-z0-9_]: Enter does not create it.
    let mut i = quiet();
    i.enter = true;
    let out = u.screen(&i, &screen);
    assert!(out.actions.is_empty(), "{:?}", out.actions);
    let mut i = quiet();
    i.backspace = 2;
    u.screen(&i, &screen);
    let mut i = quiet();
    i.typed = "Kai".into();
    u.screen(&i, &screen);
    let mut i = quiet();
    i.enter = true;
    let out = u.screen(&i, &screen);
    assert_eq!(
        out.actions,
        vec![UiAction::Create {
            name: "Kai".into(),
            look: [0; 4]
        }]
    );
}

#[test]
fn select_queue_and_random_look() {
    let mut u = ui(Locale::from_static("ko"));
    let fig = Figure {
        body: [0.2, 0.2, 0.4, 1.0],
        skin: [0.9, 0.7, 0.6, 1.0],
        hair: None,
        broad: true,
    };
    let screen = Screen::Select(SelectModel {
        characters: vec![CharacterCard {
            name: "Kai".into(),
            level: 3,
            figure: Some(fig),
        }],
        max: 5,
        ..SelectModel::default()
    });
    let mut i = quiet();
    i.enter = true;
    let out = u.screen(&i, &screen);
    assert_eq!(out.actions, vec![UiAction::Enter(0)]);
    let out = u.screen(
        &quiet(),
        &Screen::Queue {
            position: 4,
            estimate_secs: 30,
        },
    );
    assert!(out.actions.is_empty());
    let lists: [LookList; 4] = std::array::from_fn(|n| LookList {
        names: vec!["x".into(); n + 2],
        colors: vec![[0.0; 4]; n + 2],
    });
    for seed in 0..50 {
        u.screens.randomize(&lists, seed);
        for (n, v) in u.screens.look.iter().enumerate() {
            assert!(usize::from(*v) < n + 2);
        }
    }
}

#[test]
fn phase_six_hud_parts_draw_and_act() {
    let mut u = ui(Locale::EN);
    u.hud.inventory = true;
    u.hud.quests = true;
    let mut m = model();
    m.fallen = false;
    m.bag[0] = Some(BagSlot {
        name: "Bread".into(),
        count: 2,
        unique: false,
        appraised: None,
        description: "Dense bread.".into(),
        details: vec!["Use: +30 HP".into()],
        usable: true,
        icon: None,
        durability: None,
        can_sell: false,
        can_repair: false,
        can_appraise: false,
        can_self_repair: false,
        can_shop_appraise: false,
        rarity: base::Rarity::Rare,
    });
    m.quests = vec![QuestModel {
        name: "First steps".into(),
        description: "Bring three herbs.".into(),
        completed: false,
        objectives: vec![("Herb: 1/3".into(), false)],
        goal: None,
    }];
    m.overhead = vec![Overhead {
        at: (300.0, 300.0),
        name: Some("Guide".into()),
        health: Some(50),
        marker: Some(MarkerKind::Ready),
        target: false,
    }];
    m.floats = vec![
        FloatText {
            at: (320.0, 320.0),
            text: "12".into(),
            kind: FloatKind::Dealt,
            crit: true,
            age: 0.5,
            icon: None,
        },
        FloatText {
            // An emote with a picture: drawn as a bubble.
            at: (360.0, 320.0),
            text: "Wave".into(),
            kind: FloatKind::Emote,
            crit: false,
            age: 0.9,
            icon: Some([0.0, 0.0, 0.1, 0.1]),
        },
    ];
    m.dialogue = Some(("Hello.".into(), vec!["Bye.".into()]));
    m.dialogue_close = "Escape: close".into();
    // Find the first backpack slot: hover the frame until a click uses it.
    let mut used = false;
    'search: for y in (0..720).step_by(8) {
        for x in (0..1280).step_by(8) {
            let i = Input {
                mouse: (x as f32, y as f32),
                press_at: Some((x as f32, y as f32)),
                ..input()
            };
            // The scan also clicks close buttons: keep the backpack open.
            u.hud.inventory = true;
            let out = u.frame(&i, |f, hud| hud.draw(f, &m));
            if out.actions.contains(&UiAction::UseItem(0)) {
                used = true;
                break 'search;
            }
        }
    }
    assert!(used, "a click on the bread uses it");
}

#[test]
fn log_tabs_filter_lines() {
    assert!(LogTab::All.shows(LineKind::Combat));
    assert!(LogTab::Chat.shows(LineKind::Chat));
    assert!(!LogTab::Chat.shows(LineKind::Combat));
    assert!(LogTab::System.shows(LineKind::Quest));
    assert!(
        LogTab::Combat.shows(LineKind::Error),
        "errors answer the player everywhere"
    );
}

/// Pressing on hotbar slot 0, moving and letting go on slot 3 swaps them;
/// letting go away from the bar empties the slot. Neither uses it.
#[test]
fn hotbar_slots_drag_and_drop() {
    let mut u = ui(Locale::EN);
    let mut m = model();
    m.fallen = false;
    // Default sizes: 48 px slots, 52 px apart, the bar centered at the foot.
    let (slot0, slot3) = ((406.0, 688.0), (562.0, 688.0));
    let mut i = Input {
        screen: (1280.0, 720.0),
        scale: 1.0,
        mouse: slot0,
        clicked: true,
        mouse_down: true,
        press_at: Some(slot0),
        ..Input::default()
    };
    let out = u.frame(&i, |f, hud| hud.draw(f, &m));
    assert!(
        out.actions.is_empty(),
        "a press alone does nothing: {:?}",
        out.actions
    );
    i.clicked = false;
    i.mouse = (480.0, 690.0);
    u.frame(&i, |f, hud| hud.draw(f, &m));
    assert!(u.hud.dragging());
    i.mouse = slot3;
    i.mouse_down = false;
    i.released = true;
    let out = u.frame(&i, |f, hud| hud.draw(f, &m));
    assert_eq!(
        out.actions,
        vec![UiAction::HotbarDrop {
            slot: 3,
            from: DragSource::Hotbar(0)
        }]
    );
    assert!(!u.hud.dragging());

    // Out of the bar: cleared.
    i.released = false;
    i.mouse_down = true;
    i.mouse = slot0;
    u.frame(&i, |f, hud| hud.draw(f, &m));
    i.mouse = (640.0, 300.0);
    u.frame(&i, |f, hud| hud.draw(f, &m));
    i.mouse_down = false;
    i.released = true;
    let out = u.frame(&i, |f, hud| hud.draw(f, &m));
    assert_eq!(out.actions, vec![UiAction::HotbarClear(0)]);
}

/// Every window open at once, each with far more rows than fit, at three
/// screen sizes: nothing is drawn off the screen, and the wheel over a
/// window scrolls it instead of zooming the world.
#[test]
fn long_lists_stay_inside_their_windows() {
    let many = |n: usize, s: &str| (0..n).map(|i| format!("{s} {i}")).collect::<Vec<_>>();
    let mut m = model();
    m.fallen = false;
    m.sheet = SheetModel {
        free_points: 3,
        classes_available: many(6, "Class"),
        titles: many(40, "Title"),
        proficiencies: many(30, "Skill").into_iter().map(|p| (p, 7)).collect(),
        ..SheetModel::default()
    };
    m.recipes = (0..90)
        .map(|i| RecipeModel {
            id: base::RecipeId::new(i),
            name: format!("Recipe with a rather long name number {i}"),
            description: String::new(),
            icon: None,
            materials: vec![("Iron ingot".into(), 1, 3), ("Boar tusk".into(), 0, 1)],
            can: i % 2 == 0,
            blocked: "Needs smithing 5".into(),
            group: if i % 3 == 0 { "Alchemy" } else { "Smithing" }.into(),
            kind: if i % 4 == 0 {
                base::ItemKind::Weapon
            } else {
                base::ItemKind::Material
            },
            qty: 1 + i % 2,
            rarity: base::Rarity::Common,
        })
        .collect();
    m.quests = (0..30)
        .map(|i| QuestModel {
            name: format!("Quest {i}"),
            description: "A long description that wraps over more than one line of the quest log.".into(),
            completed: false,
            objectives: vec![
                ("Wolf pelt: 1/5".into(), false),
                ("Talk to the guide".into(), true),
            ],
            goal: None,
        })
        .collect();
    m.shop = Some(ShopModel {
        title: "Forge".into(),
        lines: (0..40)
            .map(|i| ShopLineModel {
                name: format!("Item {i}"),
                icon: None,
                price: 100,
                qty: Some(3),
                affordable: true,
                tooltip: vec![],
                rarity: base::Rarity::Epic,
            })
            .collect(),
        repairs: true,
        coins: "You have 120 copper".into(),
        appraises: false,
        purse: None,
        close: "Esc: close".into(),
    });
    m.help = HelpModel {
        keys: (0..40).map(|i| (format!("Action {i}"), "F1".into())).collect(),
        tips: many(
            10,
            "A tip long enough to wrap over the width of the help window, again and again",
        ),
    };
    m.log = (0..80)
        .map(|i| LogLine {
            kind: LineKind::Chat,
            text: format!("line {i}"),
        })
        .collect();
    for screen in [(1024.0, 600.0), (1280.0, 720.0), (1920.0, 1080.0)] {
        let mut u = ui(Locale::EN);
        u.hud.inventory = true;
        u.hud.sheet = true;
        u.hud.quests = true;
        u.hud.help = true;
        let quiet = Input {
            screen,
            scale: 1.0,
            mouse: (screen.0 / 2.0, screen.1 / 2.0),
            ..Input::default()
        };
        u.frame(&quiet, |f, hud| hud.draw(f, &m));
        let off: Vec<_> = u
            .vertices()
            .iter()
            .filter(|v| {
                v.pos[0] < -0.5 || v.pos[1] < -0.5 || v.pos[0] > screen.0 + 0.5 || v.pos[1] > screen.1 + 0.5
            })
            .take(3)
            .collect();
        assert!(off.is_empty(), "{screen:?}: drawn off the screen: {off:?}");

        // The crafting window takes the left side from the sheet; the wheel
        // over it scrolls it and keeps the click from the world. Without
        // the shop: on a narrow screen crafting would cover it, so it would
        // stay hidden (`hud::anim`).
        let mut m = m.clone();
        m.shop = None;
        u.hud.toggle_crafting();
        assert!(!u.hud.sheet, "status and crafting share the left side");
        let over = Input {
            wheel: -3.0,
            mouse: (60.0, 200.0),
            ..quiet.clone()
        };
        let out = u.frame(&over, |f, hud| hud.draw(f, &m));
        assert!(out.over_ui, "{screen:?}: the wheel landed on the window");
        assert_eq!(u.hud.scroll[1], 3, "{screen:?}: three rows down");
        let up = Input { wheel: 50.0, ..over };
        u.frame(&up, |f, hud| hud.draw(f, &m));
        assert_eq!(u.hud.scroll[1], 0, "{screen:?}: back to the top, not past it");
    }
}

/// Clicks every 6 px of `area` (one frame each) until `done` holds.
fn click_until(
    u: &mut Ui,
    m: &HudModel,
    area: (f32, f32, f32, f32),
    mut done: impl FnMut(&Ui) -> bool,
) -> bool {
    let (x0, y0, x1, y1) = area;
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            let i = Input {
                mouse: (x, y),
                press_at: Some((x, y)),
                ..input()
            };
            u.frame(&i, |f, hud| hud.draw(f, m));
            if done(u) {
                return true;
            }
            x += 6.0;
        }
        y += 6.0;
    }
    false
}

/// Crafting tabs and the "can make" switch, the close button of a window,
/// reading the log back with the wheel and the quest tracker.
#[test]
fn hud_helpers_answer_the_mouse() {
    let mut m = model();
    m.fallen = false;
    m.target = None;
    m.dialogue = None;
    m.recipes = (0..6)
        .map(|i| RecipeModel {
            id: base::RecipeId::new(i),
            name: format!("Recipe {i}"),
            description: String::new(),
            icon: None,
            materials: vec![("Iron ingot".into(), 1, 1)],
            can: i == 0,
            blocked: String::new(),
            group: if i < 3 { "Smithing" } else { "Alchemy" }.into(),
            kind: base::ItemKind::Weapon,
            qty: 1,
            rarity: base::Rarity::Common,
        })
        .collect();
    let mut u = ui(Locale::EN);
    u.hud.crafting = true;
    // Left of the close button, then over it.
    let body = (0.0, 0.0, 340.0, 200.0);
    assert!(
        click_until(&mut u, &m, body, |u| u.hud.craft_tab == 1),
        "second tab"
    );
    u.hud.craft_tab = 0;
    assert!(
        click_until(&mut u, &m, body, |u| u.hud.craft_ready),
        "can-make switch"
    );
    assert!(u.hud.crafting);
    let corner = (340.0, 0.0, 420.0, 200.0);
    assert!(
        click_until(&mut u, &m, corner, |u| !u.hud.crafting),
        "close button"
    );

    // The wheel over the log reads back; down again follows the newest.
    m.log = (0..40)
        .map(|i| LogLine {
            kind: LineKind::Chat,
            text: format!("line {i}"),
        })
        .collect();
    let over_log = Input {
        mouse: (40.0, 650.0),
        wheel: 3.0,
        clicked: false,
        released: false,
        press_at: None,
        ..input()
    };
    u.frame(&over_log, |f, hud| hud.draw(f, &m));
    assert_eq!(u.hud.log_scroll, 3);
    let back = Input {
        wheel: -10.0,
        ..over_log
    };
    u.frame(&back, |f, hud| hud.draw(f, &m));
    assert_eq!(u.hud.log_scroll, 0);

    // The tracker opens the quest log.
    m.quests = vec![QuestModel {
        name: "First steps".into(),
        description: String::new(),
        completed: false,
        objectives: vec![("Herb: 1/3".into(), false)],
        goal: None,
    }];
    let right = (1000.0, 150.0, 1280.0, 320.0);
    assert!(click_until(&mut u, &m, right, |u| u.hud.quests), "tracker click");
}

/// Every action a click anywhere on the screen gives, one frame per spot.
fn scan_actions(u: &mut Ui, m: &HudModel) -> Vec<UiAction> {
    let mut all = Vec::new();
    for y in (0..720).step_by(10) {
        for x in (0..1280).step_by(10) {
            let (x, y) = (x as f32, y as f32);
            let i = Input {
                mouse: (x, y),
                press_at: Some((x, y)),
                ..input()
            };
            all.extend(u.frame(&i, |f, hud| hud.draw(f, m)).actions);
        }
    }
    all
}

/// The trade window: a backpack click offers a piece, a click
/// on my offered line takes it back, Confirm and Cancel answer; the invite
/// panel accepts or declines; a slot dragged onto the window joins the offer.
#[test]
fn trade_window_offers_confirms_and_answers() {
    let mut m = model();
    m.fallen = false;
    m.target = None;
    let line = |slot, name: &str| TradeLine {
        slot,
        qty: 1,
        max: 1,
        name: name.into(),
        icon: None,
        rarity: base::Rarity::Epic,
        details: vec!["Rarity: +20% damage".into()],
    };
    m.bag[3] = Some(BagSlot {
        name: "Sword".into(),
        count: 1,
        unique: true,
        appraised: None,
        description: String::new(),
        details: Vec::new(),
        usable: false,
        icon: None,
        durability: Some((50, 50)),
        can_sell: false,
        can_repair: false,
        can_appraise: false,
        can_self_repair: false,
        can_shop_appraise: false,
        rarity: base::Rarity::Rare,
    });
    m.trade = Some(TradeModel {
        with: "Bravo".into(),
        open: true,
        asked: false,
        mine: vec![line(5, "Shield")],
        mine_coins: 7,
        theirs: vec![line(0, "Axe")],
        theirs_coins: 30,
        mine_confirmed: false,
        theirs_confirmed: true,
    });
    m.invite = Some("Charlie wants to trade.".into());
    let mut u = ui(Locale::EN);
    let actions = scan_actions(&mut u, &m);
    assert!(u.hud.inventory, "a trade opens the backpack");
    for wanted in [
        UiAction::TradeAdd(3),
        UiAction::TradeOffer {
            items: Vec::new(),
            coins: 7,
        },
        UiAction::TradeConfirm,
        UiAction::TradeCancel,
        UiAction::AnswerInvite(true),
        UiAction::AnswerInvite(false),
    ] {
        assert!(actions.contains(&wanted), "{wanted:?}");
    }
    // Equip never happens while trading: the click offers instead.
    assert!(!actions.contains(&UiAction::Equip(3)));

    // A slot dragged from the backpack and let go over the window.
    m.invite = None;
    let trade_rect = |u: &mut Ui| {
        let i = Input {
            clicked: false,
            released: false,
            press_at: None,
            mouse: (0.0, 0.0),
            ..input()
        };
        u.frame(&i, |f, hud| hud.draw(f, &m));
        u.hud.trade_rect.expect("trade drawn")
    };
    let r = trade_rect(&mut u);
    // Where the sword sits: the spot whose tap offers it.
    let at = (0..720)
        .step_by(6)
        .flat_map(|y| (0..1280).step_by(6).map(move |x| (x as f32, y as f32)))
        .find(|&(x, y)| {
            let i = Input {
                mouse: (x, y),
                press_at: Some((x, y)),
                ..input()
            };
            u.frame(&i, |f, hud| hud.draw(f, &m))
                .actions
                .contains(&UiAction::TradeAdd(3))
        })
        .expect("the sword's slot");
    let press = Input {
        mouse: (at.0 + 20.0, at.1),
        press_at: Some(at),
        mouse_down: true,
        clicked: false,
        released: false,
        ..input()
    };
    u.frame(&press, |f, hud| hud.draw(f, &m));
    let drop = Input {
        mouse: (r.x + r.w / 2.0, r.y + r.h / 2.0),
        press_at: Some(at),
        clicked: false,
        released: true,
        ..input()
    };
    let out = u.frame(&drop, |f, hud| hud.draw(f, &m));
    assert!(
        out.actions.contains(&UiAction::TradeAdd(3)),
        "drag a backpack slot onto the trade"
    );
}

/// The asked side of a trade request answers in the trade window itself
/// (the window used to say "waiting" on both sides, with no way on).
#[test]
fn asked_trade_window_accepts_and_declines() {
    let mut m = model();
    m.fallen = false;
    m.target = None;
    m.trade = Some(TradeModel {
        with: "Bravo".into(),
        asked: true,
        ..TradeModel::default()
    });
    let mut u = ui(Locale::EN);
    let actions = scan_actions(&mut u, &m);
    assert!(actions.contains(&UiAction::TradeAccept));
    assert!(actions.contains(&UiAction::TradeCancel));
}

/// Nothing social needs the chat: the group panel leaves and opens a
/// member's menu, the menu offers what the leader may do, and the emote
/// button lists the gestures.
#[test]
fn social_without_chat() {
    let mut m = model();
    m.fallen = false;
    m.target = None;
    let (me, other) = (base::EntityId::new(1), base::EntityId::new(2));
    m.party = Some(PartyModel {
        members: vec![
            PartyMember {
                id: me,
                name: "Alpha".into(),
                leader: true,
                me: true,
                health: Some(100),
            },
            PartyMember {
                id: other,
                name: "Bravo".into(),
                leader: false,
                me: false,
                health: None,
            },
        ],
        i_lead: true,
    });
    m.emotes = vec![crate::EmoteEntry {
        id: base::EmoteId::new(4),
        name: "Wave".into(),
        icon: Some([0.0, 0.0, 0.1, 0.1]),
    }];
    let mut u = ui(Locale::EN);
    let actions = scan_actions(&mut u, &m);
    assert!(actions.contains(&UiAction::LeaveParty));
    // A later click elsewhere closes the menu: look right after each one.
    let mut opened = false;
    for y in (0..720).step_by(10) {
        for x in (0..1280).step_by(10) {
            u.hud.player_menu = None;
            let (x, y) = (x as f32, y as f32);
            let i = Input {
                mouse: (x, y),
                press_at: Some((x, y)),
                ..input()
            };
            u.frame(&i, |f, hud| hud.draw(f, &m));
            opened |= u.hud.player_menu.as_ref().is_some_and(|p| p.target == other);
        }
    }
    assert!(opened, "a click on a member opens their menu");

    // The menu, open at the middle of the screen, for a member I lead.
    let mut acts = Vec::new();
    for y in (0..720).step_by(10) {
        for x in (0..1280).step_by(10) {
            u.hud.player_menu = Some(PlayerMenu {
                target: other,
                name: "Bravo".into(),
                at: (900.0, 400.0),
                fallen: true,
            });
            u.hud.emotes_open = false;
            let (x, y) = (x as f32, y as f32);
            let i = Input {
                mouse: (x, y),
                press_at: Some((x, y)),
                ..input()
            };
            acts.extend(
                u.frame(&i, |f, hud| hud.draw(f, &m))
                    .actions
                    .into_iter()
                    .filter_map(|a| match a {
                        UiAction::Player { target, act, .. } if target == other => Some(act),
                        _ => None,
                    }),
            );
        }
    }
    for want in [
        PlayerAct::Revive,
        PlayerAct::Trade,
        PlayerAct::Whisper,
        PlayerAct::Lead,
        PlayerAct::Kick,
        PlayerAct::Report,
    ] {
        assert!(acts.contains(&want), "{want:?}");
    }
    assert!(!acts.contains(&PlayerAct::Invite), "already in the group");

    // The emote button opens the list; a gesture in it is one click.
    u.hud.player_menu = None;
    m.party = None;
    let mut emoted = false;
    for y in (0..720).step_by(10) {
        for x in (0..1280).step_by(10) {
            u.hud.emotes_open = true;
            let (x, y) = (x as f32, y as f32);
            let i = Input {
                mouse: (x, y),
                press_at: Some((x, y)),
                ..input()
            };
            emoted |= u
                .frame(&i, |f, hud| hud.draw(f, &m))
                .actions
                .contains(&UiAction::Emote(base::EmoteId::new(4)));
        }
    }
    assert!(emoted, "an emote is a click away");
}

/// A stack in my offer: `-` takes one back, `+` adds one up to what the
/// slot holds, without leaving the trade window.
#[test]
fn trade_stack_quantity_is_set_on_the_line() {
    let mut m = model();
    m.fallen = false;
    m.target = None;
    m.trade = Some(TradeModel {
        with: "Bravo".into(),
        open: true,
        mine: vec![TradeLine {
            slot: 2,
            qty: 3,
            max: 5,
            name: "Arrow".into(),
            icon: None,
            rarity: base::Rarity::Common,
            details: Vec::new(),
        }],
        ..TradeModel::default()
    });
    let mut u = ui(Locale::EN);
    let actions = scan_actions(&mut u, &m);
    let offer = |qty| UiAction::TradeOffer {
        items: vec![(2, qty)],
        coins: 0,
    };
    assert!(actions.contains(&offer(2)), "minus");
    assert!(actions.contains(&offer(4)), "plus");
    assert!(
        !actions.iter().any(|a| matches!(a, UiAction::TradeOffer { items, .. } if items.iter().any(|(_, q)| *q > 5 || *q == 0))),
        "never above the stack nor zero"
    );
}

#[test]
fn dialogue_over_the_shop_takes_the_click_alone() {
    // Playtest: learning a recipe with the forge's shop open bought an item
    // under the option too, so the coins were gone and the lesson refused.
    let mut m = model();
    m.fallen = false;
    m.dialogue = Some((
        "The smith looks up from the anvil.".into(),
        (0..6).map(|i| format!("Option {i}")).collect(),
    ));
    m.shop = Some(ShopModel {
        title: "Forge".into(),
        lines: (0..12)
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
    });
    let mut chose = 0;
    let mut u = ui(Locale::EN);
    for x in (300..980).step_by(20) {
        for y in (200..620).step_by(6) {
            let at = (x as f32, y as f32);
            let click = Input {
                mouse: at,
                press_at: Some(at),
                ..input()
            };
            let out = u.frame(&click, |f, hud| hud.draw(f, &m));
            let picked = out.actions.iter().any(|a| matches!(a, UiAction::ChooseOption(_)));
            let other = out
                .actions
                .iter()
                .any(|a| matches!(a, UiAction::Buy { .. } | UiAction::Sell { .. }));
            assert!(!(picked && other), "{at:?}: {:?}", out.actions);
            chose += usize::from(picked);
        }
    }
    assert!(chose > 0, "some click reached the options");
}

fn bread() -> BagSlot {
    BagSlot {
        name: "Bread".into(),
        count: 2,
        unique: false,
        appraised: None,
        description: String::new(),
        details: vec![],
        usable: true,
        icon: None,
        durability: None,
        can_sell: true,
        can_repair: false,
        can_appraise: false,
        can_self_repair: false,
        can_shop_appraise: false,
        rarity: base::Rarity::Common,
    }
}

/// A backpack slot dragged onto another asks the server to swap them
/// (alpha: the backpack could not be reorganized).
#[test]
fn backpack_slots_drag_onto_each_other() {
    let mut u = ui(Locale::EN);
    u.hud.inventory = true;
    let mut m = model();
    m.fallen = false;
    m.bag[0] = Some(bread());
    let quiet = Input {
        mouse: (0.0, 0.0),
        clicked: false,
        released: false,
        press_at: None,
        ..input()
    };
    u.frame(&quiet, |f, hud| hud.draw(f, &m));
    let center = |r: crate::paint::Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let (a, b) = (center(u.hud.bag_rects[0]), center(u.hud.bag_rects[5]));
    let press = Input {
        mouse: (a.0 + 20.0, a.1),
        press_at: Some(a),
        mouse_down: true,
        clicked: false,
        released: false,
        ..input()
    };
    u.frame(&press, |f, hud| hud.draw(f, &m));
    assert!(u.hud.dragging());
    let drop = Input {
        mouse: b,
        press_at: Some(a),
        clicked: false,
        released: true,
        ..input()
    };
    let out = u.frame(&drop, |f, hud| hud.draw(f, &m));
    assert!(
        out.actions.contains(&UiAction::MoveItem { from: 0, to: 5 }),
        "{:?}",
        out.actions
    );
}

/// At a shop, a click on the backpack does what the chosen mode says: a
/// sale is never a repair by mistake.
#[test]
fn the_shop_mode_decides_what_a_click_does() {
    let mut m = model();
    m.fallen = false;
    m.target = None;
    m.bag[0] = Some(BagSlot {
        unique: true,
        can_repair: true,
        can_shop_appraise: true,
        durability: Some((5, 10)),
        ..bread()
    });
    m.shop = Some(ShopModel {
        title: "Forge".into(),
        lines: vec![],
        repairs: true,
        appraises: true,
        purse: Some("Pays you up to 100 more copper".into()),
        coins: "You have 120 copper".into(),
        close: "Esc".into(),
    });
    for (mode, want) in [
        (ShopMode::Sell, UiAction::Sell { slot: 0, qty: 1 }),
        (ShopMode::Repair, UiAction::Repair(0)),
        (ShopMode::Appraise, UiAction::ShopAppraise(0)),
    ] {
        let mut u = ui(Locale::EN);
        u.hud.shop_mode = mode;
        let quiet = Input {
            mouse: (0.0, 0.0),
            clicked: false,
            released: false,
            press_at: None,
            ..input()
        };
        // The shop opens the backpack; it is drawn from the next frame.
        u.frame(&quiet, |f, hud| hud.draw(f, &m));
        u.frame(&quiet, |f, hud| hud.draw(f, &m));
        let at = {
            let r = u.hud.bag_rects[0];
            (r.x + r.w / 2.0, r.y + r.h / 2.0)
        };
        let tap = Input {
            mouse: at,
            press_at: Some(at),
            ..input()
        };
        let out = u.frame(&tap, |f, hud| hud.draw(f, &m));
        assert_eq!(out.actions, vec![want], "{mode:?}");
    }
}

#[test]
fn grouped_numbers() {
    assert_eq!(crate::hud::grouped_for_tests(1_234_567), "1,234,567");
    assert_eq!(crate::hud::grouped_for_tests(999), "999");
}
