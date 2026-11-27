//! Phase 4: combat, monsters, death, progression and items with identity.

use crate::*;
use base::{CharacterName, EntityId, PlayerId, TilePos, ZoneId};
use std::sync::Arc;

/// 20x9 open field; safe square around the spawn (x 0..=3).
const MAP: &str = r#####"(
    legend: { '.': (floor: 1) },
    rows: [
        "....................",
        "....................",
        "....................",
        "....................",
        "....................",
        "....................",
        "....................",
        "....................",
        "....................",
    ],
)"#####;

const KEYS: [&str; 49] = [
    "item.bread.name",
    "item.bread.desc",
    "item.tonic.name",
    "item.tonic.desc",
    "system.item_used",
    "system.item_used_mp",
    "monster.wolf.name",
    "monster.dummy.name",
    "monster.ogre.name",
    "item.pelt.name",
    "item.pelt.desc",
    "item.torch.name",
    "item.torch.desc",
    "item.sword.name",
    "item.sword.desc",
    "proficiency.sword.name",
    "proficiency.unarmed.name",
    "class.warrior.name",
    "class.warrior.desc",
    "title.wolf_bane.name",
    "title.wolf_bane.desc",
    "title.hardy.name",
    "title.hardy.desc",
    "system.level_up",
    "system.xp",
    "loot.item",
    "loot.coins",
    "system.fallen",
    "system.died",
    "system.revived",
    "system.appraised",
    "system.appraise_failed",
    "system.title",
    "system.unique_lost",
    "monster.post.name",
    "monster.brute.name",
    "technique.quake.name",
    "technique.quake.desc",
    "system.status_poison",
    "system.status_slow",
    "system.boss_phase",
    "technique.slash.name",
    "technique.slash.desc",
    "technique.punch.name",
    "technique.punch.desc",
    "technique.bolt.name",
    "technique.bolt.desc",
    "technique.costly.name",
    "technique.costly.desc",
];

fn content() -> Arc<Content> {
    let text = format!(
        "{{ {} }}",
        KEYS.iter()
            .map(|k| format!("\"{k}\": \"x\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut files: Vec<(String, String)> = [i18n::Locale::EN]
        .iter()
        .map(|l| (format!("text/{l}/a.ron"), text.clone()))
        .collect();
    for (p, t) in [
        (
            "zones/f/zone.ron",
            "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 1, y: 1), regions: [(kind: Safe, min: (x: 0, y: 0), max: (x: 3, y: 8))])",
        ),
        ("zones/f/map.ron", MAP),
        (
            "zones/f/monsters.ron",
            "[(monster: \"wolf\", pos: (x: 12, y: 4)), (monster: \"dummy\", pos: (x: 6, y: 1)), (monster: \"ogre\", pos: (x: 18, y: 8)), (monster: \"post\", pos: (x: 6, y: 6)), (monster: \"post\", pos: (x: 8, y: 6)), (monster: \"brute\", pos: (x: 17, y: 0))]",
        ),
        // Wolf: aggressive, weak, drops a pelt and a sword.
        (
            "monsters/wolf.ron",
            "(level: 2, hp: 20, damage: (1, 2), xp: 30, aggro_radius: 3, leash: 10, respawn_ticks: 100, coins: (1, 1), loot: [(item: \"pelt\", chance: 1000), (item: \"sword\", chance: 1000)])",
        ),
        // Dummy: never attacks first (aggro 0), lots of XP.
        (
            "monsters/dummy.ron",
            "(level: 5, hp: 5, damage: (0, 0), xp: 3000, aggro_radius: 0, leash: 30, respawn_ticks: 5)",
        ),
        // Ogre: kills anyone.
        (
            "monsters/ogre.ron",
            "(level: 30, hp: 5000, damage: (500, 500), attributes: (dex: 500), xp: 1, aggro_radius: 2, leash: 4)",
        ),
        // Posts: stand still and take blows (area and poison).
        (
            "monsters/post.ron",
            "(level: 1, hp: 1000, damage: (0, 0), xp: 1, aggro_radius: 0, leash: 30)",
        ),
        // Brute: below half HP its blows slow whoever they hit.
        (
            "monsters/brute.ron",
            "(level: 1, hp: 100, damage: (1, 1), attributes: (dex: 500), xp: 1, attack_ticks: 5, aggro_radius: 1, leash: 30, phases: [(below_hp_permille: 500, status: Some((kind: Slow, duration_ticks: 200, power: 500)))])",
        ),
        ("items/pelt.ron", "(kind: Material, max_stack: 20)"),
        (
            "items/torch.ron",
            "(kind: Offhand, durability: 3, burn: 1, equip: Some(Offhand(defense: 2)), light: Some((radius: 5, color: (255, 160, 80))))",
        ),
        (
            "items/tonic.ron",
            "(kind: Consumable, max_stack: 10, use: Some((mp: 20, cooldown_ticks: 50)))",
        ),
        (
            "items/bread.ron",
            "(kind: Consumable, max_stack: 10, use: Some((hp: 30, cooldown_ticks: 50)))",
        ),
        (
            "items/sword.ron",
            "(kind: Weapon, equip: Some(Weapon(damage: (5, 8), attack_ticks: 20, attribute: Str, proficiency: \"sword\", visual: \"blade\")), hidden: [(attribute: Str, min: 3, max: 3)])",
        ),
        // Never unlocked by points: the test saves them in the sheet.
        (
            "techniques/slash.ron",
            "(proficiency: \"sword\", unlock_at: 100, cooldown_ticks: 10, damage_permille: 1500)",
        ),
        (
            "techniques/punch.ron",
            "(proficiency: \"unarmed\", unlock_at: 100, cooldown_ticks: 10, damage_permille: 1500)",
        ),
        // Costs 30 MP, barely scratches (a kill would level up and refill).
        (
            "techniques/bolt.ron",
            "(proficiency: \"unarmed\", unlock_at: 100, cooldown_ticks: 10, mp: 30, damage_permille: 1)",
        ),
        // A punch that hits everything around the target and poisons it.
        (
            "techniques/quake.ron",
            "(proficiency: \"unarmed\", unlock_at: 100, cooldown_ticks: 10, damage_permille: 1, area: 3, status: Some((kind: Poison, duration_ticks: 200, power: 5)))",
        ),
        (
            "techniques/costly.ron",
            "(proficiency: \"unarmed\", unlock_at: 100, cooldown_ticks: 10, mp: 500, damage_permille: 1500)",
        ),
        ("proficiencies/sword.ron", "(kind: Weapon)"),
        ("proficiencies/unarmed.ron", "(kind: Weapon)"),
        (
            "classes/warrior.ron",
            "(tier: 1, level: 5, auto_points: (str: 2, vit: 1))",
        ),
        ("counters.ron", "[\"kills\", \"deaths\"]"),
        (
            "titles/wolf_bane.ron",
            "(trigger: Counter(family: \"kills\", key: \"wolf\", at: 1), announce: First, unique: Some((heir_after_days: 7)))",
        ),
        (
            "titles/hardy.ron",
            "(trigger: Attribute(Vit, 20), bonuses: [MaxHp(50)])",
        ),
    ] {
        files.push((p.into(), t.into()));
    }
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    Arc::new(Content::new(defs::from_text(&refs).unwrap().validate().unwrap()))
}

struct Run {
    zone: Zone,
    out: TickOutput,
}

impl Run {
    fn new() -> Run {
        Run {
            zone: Zone::new(content(), ZoneId::new(0), 7).unwrap(),
            out: TickOutput::default(),
        }
    }

    fn tick(&mut self, commands: &[Command]) {
        let input = TickInput {
            commands,
            now_unix: 1_000,
            load: LoadLevel::N0,
        };
        self.zone.tick(&input, &mut self.out);
    }

    fn run(&mut self, n: usize) {
        for _ in 0..n {
            self.tick(&[]);
        }
    }

    fn monster(&self, name: &str) -> Option<EntityId> {
        let (mid, _) = self.zone.content().catalog.monster_by_name(name)?;
        self.zone.monster_ids(mid).first().copied()
    }

    fn sheet(&self, player: u32) -> CharacterSheet {
        self.zone
            .players()
            .into_iter()
            .find(|(p, _, _)| *p == PlayerId::new(player))
            .unwrap()
            .1
    }
}

fn enter_at(player: u32, pos: TilePos, sheet: Option<CharacterSheet>) -> Command {
    Command::Enter {
        player: PlayerId::new(player),
        sheet: Box::new(
            sheet.unwrap_or_else(|| CharacterSheet::new(CharacterName::try_from("hero").unwrap())),
        ),
        pos: Some(pos),
    }
}

const P1: PlayerId = PlayerId::new(1);

#[test]
fn kill_gives_xp_loot_counter_title_and_respawns() {
    let mut r = Run::new();
    r.tick(&[
        enter_at(1, TilePos::new(11, 4), None),
        Command::ItemIds {
            start: 100,
            end: 1000,
        },
    ]);
    let wolf = r.monster("wolf").unwrap();
    r.tick(&[Command::Attack {
        player: P1,
        target: wolf,
    }]);
    let mut killed = false;
    for _ in 0..600 {
        r.tick(&[]);
        if r.out.combat.iter().any(|c| c.target == wolf && c.killed) {
            killed = true;
            break;
        }
    }
    assert!(killed, "the wolf dies");
    let s = r.sheet(1);
    assert_eq!(s.progress.counters, vec![("kills".into(), "wolf".into(), 1)]);
    assert_eq!(s.progress.titles, vec!["wolf_bane".to_owned()]);
    assert_eq!(s.progress.xp, 30);
    assert_eq!(s.coins, 1);
    assert!(s.backpack.iter().flatten().any(|b| b.item == "pelt"));
    assert_eq!(s.items.len(), 1, "the sword is an item with identity");
    assert_eq!(s.items[0].id, 100);
    assert!(s.progress.proficiencies.iter().any(|(p, _, _)| p == "unarmed"));
    assert!(r.monster("wolf").is_none());
    r.run(101);
    assert!(r.monster("wolf").is_some(), "respawned");
}

#[test]
fn a_title_added_later_is_granted_on_entering() {
    // The counter was saved before `wolf_bane` existed: entering grants it.
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.progress.counters = vec![("kills".into(), "wolf".into(), 3)];
    let mut r = Run::new();
    r.tick(&[enter_at(1, TilePos::new(11, 4), Some(sheet))]);
    assert_eq!(r.sheet(1).progress.titles, vec!["wolf_bane".to_owned()]);
    assert!(
        r.out.system.iter().any(|(p, _)| *p == P1),
        "the System announces the title"
    );
}

#[test]
fn equip_appraise_and_save_keep_one_item() {
    let mut r = Run::new();
    r.tick(&[
        enter_at(1, TilePos::new(11, 4), None),
        Command::ItemIds { start: 5, end: 6 },
    ]);
    let wolf = r.monster("wolf").unwrap();
    r.tick(&[Command::Attack {
        player: P1,
        target: wolf,
    }]);
    r.run(600);
    let s = r.sheet(1);
    let ItemPlace::Backpack(slot) = s.items[0].place else {
        panic!("in the backpack")
    };
    assert!(r.out.need_item_ids || r.zone.snapshot().item_ids == Some((6, 6)));
    r.tick(&[Command::Equip { player: P1, slot }]);
    let s = r.sheet(1);
    assert_eq!(s.items[0].place, ItemPlace::Equipped(EquipSlot::Weapon));
    assert_eq!(s.backpack.get(usize::from(slot)).cloned().flatten(), None);
    // Out and in again: the same item, still one.
    r.tick(&[Command::Leave { player: P1 }]);
    let saved = r.out.left[0].1.clone();
    assert_eq!(saved.items.len(), 1);
    r.tick(&[enter_at(1, TilePos::new(2, 2), Some(saved.clone()))]);
    let again = r.sheet(1);
    assert_eq!(again.items, saved.items);
    // The weapon draws on the body.
    r.tick(&[Command::Unequip {
        player: P1,
        slot: EquipSlot::Weapon,
    }]);
    let s = r.sheet(1);
    let ItemPlace::Backpack(slot) = s.items[0].place else {
        panic!()
    };
    // Without a production class the piece is not appraised.
    r.tick(&[Command::Appraise { player: P1, slot }]);
    assert!(r.out.refused.contains(&(P1, Refusal::RequirementsNotMet)));
    assert!(!r.sheet(1).items[0].appraised);
}

#[test]
fn a_torch_in_the_off_hand_shines_and_burns_down() {
    let mut r = Run::new();
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.items.push(SavedItem {
        id: 9,
        item: "torch".into(),
        place: ItemPlace::Equipped(EquipSlot::Offhand),
        hidden: Vec::new(),
        appraised: false,
        durability: (3, 3),
        crafter: None,
        rarity: None,
        mastery: 0,
        power: None,
    });
    r.tick(&[enter_at(1, TilePos::new(2, 2), Some(sheet))]);
    let me = r.out.entered[0].1;
    let torch = r.zone.content().catalog.item_by_name("torch").unwrap().0;
    let state = r.zone.entity_state(me).unwrap();
    // The light is the torch's (ItemId + 1); off-hand defense counts.
    assert_eq!(state.light, torch.get() + 1);
    // Three minutes burn three points: the torch is gone, and its light.
    r.run(3 * 60 * 20 + 1);
    assert!(r.sheet(1).items.is_empty());
    assert_eq!(r.zone.entity_state(me).unwrap().light, 0);
}

#[test]
fn safe_ground_and_pvp_rules() {
    let mut r = Run::new();
    r.tick(&[
        enter_at(1, TilePos::new(1, 1), None),
        enter_at(2, TilePos::new(2, 1), None),
    ]);
    let two = r.zone.players().len();
    assert_eq!(two, 2);
    let bob = r.out.entered[1].1;
    r.tick(&[Command::Attack {
        player: P1,
        target: bob,
    }]);
    assert_eq!(r.out.refused, vec![(P1, Refusal::InvalidTarget)]);
}

#[test]
fn newbie_falls_and_returns_without_loss() {
    let mut r = Run::new();
    r.tick(&[enter_at(1, TilePos::new(17, 8), None)]);
    let mut fell = r.out.stats.iter().any(|(_, s)| s.fallen);
    for _ in 0..200 {
        r.tick(&[]);
        if r.out.stats.iter().any(|(_, s)| s.fallen) {
            fell = true;
            break;
        }
    }
    assert!(fell, "the ogre fells the player");
    r.tick(&[Command::Move {
        player: P1,
        destination: TilePos::new(10, 4),
    }]);
    r.tick(&[Command::GiveUp { player: P1 }]);
    let (_, s, pos) = r.zone.players().into_iter().next().unwrap();
    assert!(pos.x <= 3, "back at the spawn: {pos:?}");
    assert_eq!(s.progress.level, 1);
    assert_eq!(s.locked_until, 0);
    assert_eq!(s.progress.counters, vec![("deaths".into(), String::new(), 1)]);
}

#[test]
fn level_one_to_ten_class_and_hard_death() {
    let mut r = Run::new();
    r.tick(&[enter_at(1, TilePos::new(5, 1), None)]);
    let mut kills = 0;
    while r.sheet(1).progress.level < 11 && kills < 40 {
        let Some(d) = r.monster("dummy") else {
            r.run(6);
            continue;
        };
        r.tick(&[Command::Attack {
            player: P1,
            target: d,
        }]);
        for _ in 0..200 {
            r.tick(&[]);
            if r.out.combat.iter().any(|c| c.target == d && c.killed) {
                kills += 1;
                break;
            }
        }
    }
    let s = r.sheet(1);
    assert!(s.progress.level >= 10, "level {}", s.progress.level);
    assert!(s.progress.free_points > 0);
    let (warrior, _) = r.zone.content().catalog.class_by_name("warrior").unwrap();
    r.tick(&[Command::ChooseClass {
        player: P1,
        class: warrior,
    }]);
    assert_eq!(r.sheet(1).progress.class.as_deref(), Some("warrior"));
    let points = r.sheet(1).progress.free_points;
    let vit = r.sheet(1).progress.attributes.vit;
    assert!(vit < 20 && vit + points >= 20, "vit {vit}, points {points}");
    assert!(!r.sheet(1).progress.titles.contains(&"hardy".to_owned()));
    r.tick(&[Command::AllocatePoints {
        player: P1,
        points: defs::Attributes {
            vit: points,
            ..Default::default()
        },
    }]);
    assert_eq!(r.sheet(1).progress.free_points, 0);
    assert!(
        r.sheet(1).progress.titles.contains(&"hardy".to_owned()),
        "spending points earns an attribute title"
    );
    let (_, view) = r.out.stats.iter().find(|(p, _)| *p == P1).unwrap();
    let formulas = r.zone.content().catalog.formulas();
    assert_eq!(
        view.max_hp,
        formulas.max_hp(&view.attributes, view.level) + 50,
        "the title adds its HP at once"
    );
    // Past the newbie level a death costs a level and locks the character.
    let level = r.sheet(1).progress.level;
    r.tick(&[Command::Move {
        player: P1,
        destination: TilePos::new(17, 7),
    }]);
    let mut dead = false;
    for _ in 0..3000 {
        r.tick(&[]);
        if r.out.dead.contains(&P1) {
            dead = true;
            break;
        }
    }
    assert!(level > 10 && dead, "level {level}, dead {dead}");
    if dead {
        let s = r.sheet(1);
        assert_eq!(s.progress.level, level - 1);
        assert_eq!(s.locked_until, 1_000 + 6 * 3600);
    }
}

#[test]
fn combat_is_deterministic() {
    let run = || {
        let mut r = Run::new();
        r.tick(&[
            enter_at(1, TilePos::new(11, 4), None),
            Command::ItemIds { start: 1, end: 50 },
        ]);
        let wolf = r.monster("wolf").unwrap();
        r.tick(&[Command::Attack {
            player: P1,
            target: wolf,
        }]);
        r.run(300);
        r.zone.state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn snapshot_restores_combat_state() {
    let mut r = Run::new();
    r.tick(&[
        enter_at(1, TilePos::new(11, 4), None),
        Command::ItemIds { start: 1, end: 50 },
    ]);
    let wolf = r.monster("wolf").unwrap();
    r.tick(&[Command::Attack {
        player: P1,
        target: wolf,
    }]);
    r.run(30);
    let state = r.zone.snapshot();
    let z = Zone::from_snapshot(content(), ZoneId::new(0), 7, &state).unwrap();
    assert_eq!(z.state_hash(), r.zone.state_hash());
}

#[test]
fn eating_restores_hp_once_per_cooldown() {
    let mut r = Run::new();
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.hp = 10;
    sheet.backpack = vec![Some(SavedStack {
        item: "bread".into(),
        qty: 2,
    })];
    r.tick(&[enter_at(1, TilePos::new(1, 1), Some(sheet))]);
    let eat = Command::UseItem { player: P1, slot: 0 };
    r.tick(std::slice::from_ref(&eat));
    assert!(r.out.refused.is_empty());
    assert_eq!(r.out.stats[0].1.hp, 40);
    assert_eq!(r.out.inventories[0].1.slots[0].map(|s| s.1), Some(1));
    assert_eq!(r.out.notes.len(), 1, "a log line, no System window");
    assert!(r.out.system.is_empty());
    // Resting: refused, the bread is kept.
    r.tick(std::slice::from_ref(&eat));
    assert_eq!(r.out.refused, vec![(P1, Refusal::NotReady)]);
    assert!(r.out.inventories.is_empty());
    // A slot without a usable item.
    r.tick(&[Command::UseItem { player: P1, slot: 5 }]);
    assert_eq!(r.out.refused, vec![(P1, Refusal::InvalidSlot)]);
    r.run(50);
    r.tick(&[eat]);
    assert!(r.out.refused.is_empty());
    assert_eq!(r.sheet(1).backpack[0], None);
}

#[test]
fn xp_goes_to_the_log_not_to_a_window() {
    let mut r = Run::new();
    r.tick(&[enter_at(1, TilePos::new(5, 1), None)]);
    let dummy = r.monster("dummy").unwrap();
    r.tick(&[Command::Attack {
        player: P1,
        target: dummy,
    }]);
    let (mut notes, mut windows) = (0, 0);
    for _ in 0..200 {
        r.tick(&[]);
        notes += r.out.notes.len();
        windows += r.out.system.len();
    }
    assert!(notes > 0, "+XP in the log");
    assert!(windows > 0, "level up in a window");
}

#[test]
fn moving_away_drops_the_target() {
    let mut r = Run::new();
    r.tick(&[enter_at(1, TilePos::new(5, 4), None)]);
    let wolf = r.monster("wolf").unwrap();
    r.tick(&[Command::Attack {
        player: P1,
        target: wolf,
    }]);
    r.run(8);
    let chasing = r.zone.position_of(P1).unwrap();
    assert!(chasing.x > 5, "the weapon walks the player in reach: {chasing:?}");
    // Run off (out of the safe square, before any blow): the player must
    // not walk back once there.
    let away = TilePos::new(5, 8);
    r.tick(&[Command::Move {
        player: P1,
        destination: away,
    }]);
    r.run(200);
    assert_eq!(r.zone.position_of(P1), Some(away));
}

#[test]
fn no_retaliation_right_after_a_move() {
    let mut r = Run::new();
    // Next to the wolf's aggro radius; one step in.
    r.tick(&[enter_at(1, TilePos::new(8, 4), None)]);
    let me = r.out.entered[0].1;
    r.tick(&[Command::Move {
        player: P1,
        destination: TilePos::new(9, 4),
    }]);
    let mut hit_back = false;
    for _ in 0..30 {
        r.tick(&[]);
        hit_back |= r.out.combat.iter().any(|c| c.attacker == me);
    }
    assert!(!hit_back, "fleeing wins for 2 s");
    // Standing still, the player retaliates in reach.
    for _ in 0..200 {
        r.tick(&[]);
        hit_back |= r.out.combat.iter().any(|c| c.attacker == me);
    }
    assert!(hit_back, "idle players hit back");
}

#[test]
fn a_technique_needs_the_weapon_of_its_proficiency() {
    let mut r = Run::new();
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.progress.techniques = vec![("slash".into(), 1, 0), ("punch".into(), 1, 0)];
    r.tick(&[enter_at(1, TilePos::new(6, 2), Some(sheet))]);
    let cat = r.zone.content().clone();
    let id = |n: &str| cat.catalog.technique_by_name(n).unwrap().0;
    let dummy = r.monster("dummy").unwrap();
    let use_it = |technique, target| Command::UseTechnique {
        player: P1,
        technique,
        target,
    };

    // Empty hands: the sword technique is refused, the punch is not.
    r.tick(&[use_it(id("slash"), dummy)]);
    assert_eq!(r.out.refused, vec![(P1, Refusal::RequirementsNotMet)]);
    r.tick(&[use_it(id("punch"), dummy)]);
    assert!(r.out.refused.is_empty(), "{:?}", r.out.refused);

    // A sword in hand: the other way round.
    r.tick(&[Command::Leave { player: P1 }]);
    let mut saved = r.out.left[0].1.clone();
    saved.items.push(SavedItem {
        id: 50,
        item: "sword".into(),
        place: ItemPlace::Equipped(EquipSlot::Weapon),
        hidden: Vec::new(),
        appraised: false,
        durability: (40, 40),
        crafter: None,
        rarity: None,
        mastery: 0,
        power: None,
    });
    r.tick(&[enter_at(1, TilePos::new(6, 2), Some(saved))]);
    r.run(20);
    let dummy = r.monster("dummy").unwrap();
    r.tick(&[use_it(id("punch"), dummy)]);
    assert_eq!(r.out.refused, vec![(P1, Refusal::RequirementsNotMet)]);
    r.tick(&[use_it(id("slash"), dummy)]);
    assert!(r.out.refused.is_empty(), "{:?}", r.out.refused);
}

#[test]
fn a_spell_spends_mp_and_it_regenerates() {
    let mut r = Run::new();
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.progress.techniques = vec![("bolt".into(), 1, 0), ("costly".into(), 1, 0)];
    r.tick(&[enter_at(1, TilePos::new(6, 2), Some(sheet))]);
    let cat = r.zone.content().clone();
    let id = |n: &str| cat.catalog.technique_by_name(n).unwrap().0;
    let dummy = r.monster("dummy").unwrap();
    let use_it = |technique| Command::UseTechnique {
        player: P1,
        technique,
        target: dummy,
    };
    let mp = |r: &Run| {
        r.out
            .stats
            .iter()
            .rev()
            .find(|(p, _)| *p == P1)
            .map(|(_, s)| (s.mp, s.max_mp))
    };

    // More MP than the pool: refused, nothing spent.
    r.tick(&[use_it(id("costly"))]);
    assert_eq!(r.out.refused, vec![(P1, Refusal::NotReady)]);

    // Affordable: spent now, regained a bit at a time.
    r.tick(&[use_it(id("bolt"))]);
    assert!(r.out.refused.is_empty(), "{:?}", r.out.refused);
    let (now, max) = mp(&r).expect("stats sent after the cast");
    assert_eq!(max - now, 30);
    let mut later = now;
    for _ in 0..60 {
        r.tick(&[]);
        if let Some(v) = mp(&r) {
            later = v.0;
        }
    }
    assert!(later > now, "mp {now} -> {later}");
}

#[test]
fn a_tonic_restores_mp_and_is_kept_when_full() {
    let mut r = Run::new();
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.progress.techniques = vec![("bolt".into(), 1, 0)];
    sheet.backpack = vec![Some(SavedStack {
        item: "tonic".into(),
        qty: 2,
    })];
    r.tick(&[enter_at(1, TilePos::new(6, 2), Some(sheet))]);
    let drink = Command::UseItem { player: P1, slot: 0 };
    // Full MP: nothing to restore, the tonic is kept.
    r.tick(std::slice::from_ref(&drink));
    assert_eq!(r.out.refused, vec![(P1, Refusal::NotReady)]);
    let bolt = r.zone.content().catalog.technique_by_name("bolt").unwrap().0;
    let dummy = r.monster("dummy").unwrap();
    r.tick(&[Command::UseTechnique {
        player: P1,
        technique: bolt,
        target: dummy,
    }]);
    let spent = r
        .out
        .stats
        .iter()
        .rev()
        .find(|(p, _)| *p == P1)
        .map(|(_, s)| s.mp)
        .unwrap();
    r.tick(&[drink]);
    assert!(r.out.refused.is_empty(), "{:?}", r.out.refused);
    let after = r
        .out
        .stats
        .iter()
        .rev()
        .find(|(p, _)| *p == P1)
        .map(|(_, s)| s.mp)
        .unwrap();
    assert!(after >= spent + 20, "mp {spent} -> {after}");
    assert_eq!(r.out.inventories[0].1.slots[0].map(|s| s.1), Some(1));
}

/// Hidden attributes come on a few common pieces only, and never as a
/// zero (: the alpha found them on almost every piece).
#[test]
fn few_common_pieces_carry_hidden_attributes() {
    use crate::gear::ItemInstance;
    let content = content();
    let cat = &content.catalog;
    let sword = cat.item_by_name("sword").unwrap().0;
    let mut ids = base::IdBlock::new(1, 10_000);
    let mut rng = base::Rng64::new(7);
    let mut with = 0;
    for _ in 0..2000 {
        let inst = ItemInstance::create(sword, cat, &mut ids, &mut rng).unwrap();
        assert!(inst.hidden.iter().all(|(_, v)| *v > 0));
        with += usize::from(!inst.hidden.is_empty());
    }
    // 10% of common pieces, give or take.
    assert!((120..=280).contains(&with), "{with} of 2000");
}

/// The new-game template: a fresh character with no weapon fights a wolf
/// with bare hands, and the blows land.
#[test]
fn a_new_character_of_the_template_hurts_a_wolf() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/new-game/content");
    let c = Arc::new(Content::new(defs::load(&root).unwrap().validate().unwrap()));
    let meadow = c.catalog.zone_by_name("meadow").unwrap().0;
    let mut r = Run {
        zone: Zone::new(c, meadow, 7).unwrap(),
        out: TickOutput::default(),
    };
    r.tick(&[enter_at(1, TilePos::new(30, 9), None)]);
    let wolf = r.monster("wolf").unwrap();
    r.tick(&[Command::Attack {
        player: P1,
        target: wolf,
    }]);
    let (mut swings, mut landed) = (0, 0);
    for _ in 0..400 {
        r.tick(&[]);
        for e in r.out.combat.iter().filter(|e| e.target == wolf) {
            swings += 1;
            if e.damage.is_some_and(|d| d > 0) {
                landed += 1;
            }
        }
    }
    assert!(swings > 0, "the character swings at the wolf");
    assert!(landed > 0, "some blows hurt it");
    let s = r.sheet(1);
    assert!(
        s.progress.proficiencies.iter().any(|(p, _, _)| p == "unarmed"),
        "bare hands train the template's unarmed proficiency"
    );
}

#[test]
fn a_unique_announced_title_is_claimed_and_can_be_lost() {
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.progress.counters = vec![("kills".into(), "wolf".into(), 3)];
    let mut r = Run::new();
    // Earned on entering: the server hears of it and the gateway claims it.
    r.tick(&[enter_at(1, TilePos::new(1, 1), Some(sheet.clone()))]);
    let (tid, _) = r.zone.content().catalog.title_by_name("wolf_bane").unwrap();
    assert_eq!(r.out.feats, vec![(P1, tid)]);
    assert_eq!(r.out.claims, vec![(P1, "title:wolf_bane".to_owned())]);
    // Someone else was first: it goes, with a System line.
    r.tick(&[Command::Revoke {
        player: P1,
        key: "title:wolf_bane".into(),
    }]);
    assert!(r.sheet(1).progress.titles.is_empty());
    assert_eq!(r.out.system.len(), 1);
    // A player told before entering that it is taken never gets it.
    let p2 = PlayerId::new(2);
    r.tick(&[
        Command::Taken {
            player: p2,
            keys: vec!["title:wolf_bane".into()],
        },
        enter_at(2, TilePos::new(2, 1), Some(sheet)),
    ]);
    assert!(r.sheet(2).progress.titles.is_empty());
    assert!(r.out.claims.is_empty());
}

#[test]
fn an_area_blow_poisons_every_post_around_the_target() {
    let mut r = Run::new();
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.progress.techniques = vec![("quake".into(), 1, 0)];
    sheet.progress.attributes.dex = 500;
    r.tick(&[enter_at(1, TilePos::new(7, 7), Some(sheet))]);
    let cat = r.zone.content().clone();
    let (quake, _) = cat.catalog.technique_by_name("quake").unwrap();
    let (post, _) = cat.catalog.monster_by_name("post").unwrap();
    let posts = r.zone.monster_ids(post);
    assert_eq!(posts.len(), 2);
    r.tick(&[Command::UseTechnique {
        player: P1,
        technique: quake,
        target: posts[0],
    }]);
    assert!(r.out.refused.is_empty(), "{:?}", r.out.refused);
    let hit: Vec<_> = r
        .out
        .combat
        .iter()
        .filter(|c| c.technique == Some(quake))
        .map(|c| c.target)
        .collect();
    assert_eq!(hit, posts, "the target and the post beside it");
    let hp = |r: &Run, e| {
        let h = r.zone.by_id[&e];
        r.zone.world.get::<&fight::Fighter>(h).unwrap().hp
    };
    let before = [hp(&r, posts[0]), hp(&r, posts[1])];
    r.run(41);
    // Two bites of 5 HP each, on both (the target also takes the punches
    // that follow).
    assert_eq!(hp(&r, posts[1]), before[1] - 10);
    assert!(hp(&r, posts[0]) <= before[0] - 10);
}

#[test]
fn a_boss_changes_phase_below_half_and_slows() {
    let mut r = Run::new();
    r.tick(&[enter_at(1, TilePos::new(16, 0), None)]);
    let cat = r.zone.content().clone();
    let (brute, _) = cat.catalog.monster_by_name("brute").unwrap();
    let id = r.zone.monster_ids(brute)[0];
    let h = r.zone.by_id[&id];
    r.zone.world.get::<&mut fight::Fighter>(h).unwrap().hp = 40;
    let mut told = false;
    for _ in 0..40 {
        r.tick(&[]);
        told |= !r.out.system.is_empty();
    }
    assert!(told, "the player hears of the phase");
    assert_eq!(r.zone.world.get::<&fight::MonsterComp>(h).unwrap().phase, 1);
    let ph = r.zone.by_player[&P1];
    let now = r.zone.tick.0;
    assert!(
        r.zone
            .world
            .get::<&fight::Fighter>(ph)
            .unwrap()
            .status(defs::StatusKind::Slow, now)
            .is_some()
    );
}
