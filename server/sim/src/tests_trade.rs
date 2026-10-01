//! Phase 7 and 8: shop, durability and repair, gathering and crafting.
//! Content lives in the test itself, so a balance change in `content/`
//! never breaks these.

use crate::types::{CoinFlow, Command, EquipSlot, Refusal, TickInput, TickOutput};
use crate::{CharacterSheet, Content, LoadLevel, Zone};
use base::{CharacterName, EntityId, PlayerId, Rarity, TilePos, ZoneId};
use std::sync::Arc;

pub(crate) const MAP: &str = r#####"(
    legend: { '.': (floor: 1), 'w': (floor: 3, blocks_movement: true, blocks_sight: true) },
    rows: [
        "wwwwwwwwww",
        "w........w",
        "w........w",
        "w........w",
        "wwwwwwwwww",
    ],
)"#####;

const KEYS: [&str; 40] = [
    "proficiency.light.name",
    "technique.heal.name",
    "technique.heal.desc",
    "technique.raise.name",
    "technique.raise.desc",
    "item.draught.name",
    "item.draught.desc",
    "emote.wave.name",
    "item.ingot.name",
    "item.ingot.desc",
    "item.sword.name",
    "item.sword.desc",
    "item.relic.name",
    "item.relic.desc",
    "item.bread.name",
    "item.bread.desc",
    "item.pelt.name",
    "item.pelt.desc",
    "monster.dummy.name",
    "npc.smith.name",
    "node.vein.name",
    "recipe.sword.name",
    "recipe.sword.desc",
    "proficiency.smithing.name",
    "proficiency.mining.name",
    "proficiency.unarmed.name",
    "class.artisan.name",
    "class.artisan.desc",
    "system.bought",
    "system.sold",
    "system.repaired",
    "system.crafted",
    "system.item_broken",
    "technique.vow.name",
    "technique.vow.desc",
    "system.not_enough_coins",
    "system.self_repaired",
    "system.self_repair_class_only",
    "system.craft_material",
    "system.vow_taken",
];

fn content() -> Arc<Content> {
    content_with(
        "(level: 1, hp: 100000, damage: (0, 0), xp: 1, aggro_radius: 0, leash: 30, respawn_ticks: 5)",
    )
}

pub(crate) fn content_with(dummy: &str) -> Arc<Content> {
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
            // The bottom row's left corner is a workshop.
            "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 1, y: 1), regions: [(kind: Workshop, min: (x: 1, y: 3), max: (x: 3, y: 3))])",
        ),
        ("zones/f/map.ron", MAP),
        ("zones/f/npcs.ron", "[(npc: \"smith\", pos: (x: 2, y: 1))]"),
        ("zones/f/nodes.ron", "[(node: \"vein\", pos: (x: 4, y: 1))]"),
        (
            "zones/f/monsters.ron",
            "[(monster: \"dummy\", pos: (x: 6, y: 1))]",
        ),
        // A dummy that never attacks and never dies: the sword wears out on it.
        ("monsters/dummy.ron", dummy),
        ("npcs/smith.ron", "(shop: \"forge\")"),
        ("emotes/wave.ron", "(cooldown_ticks: 40)"),
        (
            "shops/forge.ron",
            "(repairs: true, appraises: true, purse: Some((coins: 100000, refill_secs: 3600)), stock: [(item: \"bread\", qty: 0), (item: \"sword\", qty: 1, restock_ticks: 50), (item: \"ingot\", qty: 10)])",
        ),
        (
            "items/bread.ron",
            "(kind: Consumable, max_stack: 10, value: 10, use: Some((hp: 5, cooldown_ticks: 10)))",
        ),
        ("items/ingot.ron", "(kind: Material, max_stack: 20, value: 100)"),
        // Stands a fallen player up with 30% of their HP.
        (
            "items/draught.ron",
            "(kind: Consumable, max_stack: 5, value: 50, use: Some((revive_permille: 300, cooldown_ticks: 100)))",
        ),
        ("items/pelt.ron", "(kind: Material, max_stack: 20, value: 40)"),
        (
            "items/sword.ron",
            "(kind: Weapon, value: 1000, durability: 40, equip: Some(Weapon(damage: (5, 8), attack_ticks: 20, attribute: Str, proficiency: \"smithing\")))",
        ),
        // A mythic blade: always rolls its one power.
        (
            "items/relic.ron",
            "(kind: Weapon, rarity: Mythic, value: 1000, durability: 40, powers: [Attribute(Dex, 3)], equip: Some(Weapon(damage: (10, 20), attack_ticks: 20, attribute: Str, proficiency: \"smithing\")))",
        ),
        ("proficiencies/unarmed.ron", "(kind: Weapon)"),
        ("proficiencies/smithing.ron", "(kind: Production)"),
        ("proficiencies/mining.ron", "(kind: Production)"),
        // Light magic: no weapon in hand.
        ("proficiencies/light.ron", "(kind: Magic)"),
        (
            "techniques/heal.ron",
            "(proficiency: \"light\", unlock_at: 1, cooldown_ticks: 10, mp: 5, heal_permille: 200, range: 6)",
        ),
        // A vow, defense +50% (at least 4) for 100 ticks.
        (
            "techniques/vow.ron",
            "(proficiency: \"light\", unlock_at: 1, cooldown_ticks: 200, vigor: 5, buff: Some((stat: Defense, permille: 500, flat: 4, duration_ticks: 100)), taught_only: true)",
        ),
        (
            "techniques/raise.ron",
            "(proficiency: \"light\", unlock_at: 75, cooldown_ticks: 100, mp: 10, revive_permille: 500, range: 3)",
        ),
        (
            "classes/artisan.ron",
            "(tier: 1, level: 1, craft_xp_permille: 600, quality_cap: Epic)",
        ),
        (
            "recipes/sword.ron",
            "(proficiency: \"smithing\", materials: [(item: \"ingot\", qty: 2)], result: \"sword\", xp: 50)",
        ),
        (
            "loot/vein.ron",
            "(entries: [(item: \"ingot\", chance: 1000, qty: (2, 2))])",
        ),
        (
            "nodes/vein.ron",
            "(proficiency: \"mining\", cooldown_ticks: 30, loot: \"vein\", xp: 5)",
        ),
        // A shop that pays and charges the same would be free money: keep
        // the defaults of `economy/formulas.ron` (sells at 100%, buys at 30%).
    ] {
        files.push((p.into(), t.into()));
    }
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    Arc::new(Content::new(defs::from_text(&refs).unwrap().validate().unwrap()))
}

const P1: PlayerId = PlayerId::new(1);

struct Run {
    zone: Zone,
    out: TickOutput,
}

impl Run {
    fn new() -> Run {
        Run {
            zone: Zone::new(content(), ZoneId::new(0), 11).unwrap(),
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
        assert!(self.zone.index_error().is_none(), "indexes follow the world");
    }

    /// Enters with `coins` and the materials a craft needs.
    fn enter(&mut self, coins: u64) {
        let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
        sheet.coins = coins;
        self.tick(&[
            Command::Enter {
                player: P1,
                sheet: Box::new(sheet),
                pos: Some(TilePos::new(1, 1)),
            },
            Command::ItemIds { start: 1, end: 100 },
        ]);
    }

    fn sheet(&self) -> CharacterSheet {
        self.zone
            .players()
            .into_iter()
            .find(|(p, _, _)| *p == P1)
            .unwrap()
            .1
    }

    fn entity_of<T: hecs::Component>(&self) -> EntityId {
        *self
            .zone
            .by_id
            .iter()
            .find(|(_, h)| self.zone.world.get::<&T>(**h).is_ok())
            .unwrap()
            .0
    }

    /// The player's entity.
    fn sheet_id(&self) -> EntityId {
        let h = *self.zone.by_player.get(&P1).unwrap();
        self.zone.world.get::<&crate::zone::Body>(h).unwrap().id
    }

    fn refusals(&self) -> Vec<Refusal> {
        self.out.refused.iter().map(|(_, r)| *r).collect()
    }

    fn stack(&self, item: &str) -> u32 {
        self.sheet()
            .backpack
            .iter()
            .flatten()
            .filter(|s| s.item == item)
            .map(|s| s.qty)
            .sum()
    }
}

#[test]
fn buying_and_selling_move_coins_and_leave_a_trail() {
    let mut r = Run::new();
    r.enter(3_000);
    let smith = r.entity_of::<crate::zone::NpcComp>();
    // Too far: the shop does not open.
    r.tick(&[Command::OpenShop {
        player: P1,
        target: smith,
    }]);
    assert_eq!(r.refusals(), vec![]);
    let view = r.out.shops.first().cloned().unwrap().1.unwrap();
    assert_eq!(view.npc, smith);
    assert_eq!(view.stock.len(), 3);
    // Bread at 100% of its value, unlimited; the sword has one piece.
    assert_eq!(view.stock[0].price, 10);
    assert_eq!(view.stock[0].qty, None);
    assert_eq!(view.stock[1].qty, Some(1));
    assert_eq!(view.pays_permille, 300);

    // Five breads: 50 copper out of the world.
    r.tick(&[Command::Buy {
        player: P1,
        index: 0,
        qty: 5,
    }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(r.sheet().coins, 2_950);
    assert_eq!(r.stack("bread"), 5);
    assert_eq!(r.out.coins, vec![(CoinFlow::ShopPurchase, 50)]);

    // Selling pays 30% of the value.
    r.tick(&[Command::Sell {
        player: P1,
        slot: slot_of(&r, "bread"),
        qty: 2,
    }]);
    assert_eq!(r.sheet().coins, 2_950 + 2 * 3);
    assert_eq!(r.stack("bread"), 3);
    assert_eq!(r.out.coins, vec![(CoinFlow::ShopSale, 6)]);

    // The only sword leaves the stock and does not come back at once.
    r.tick(&[Command::Buy {
        player: P1,
        index: 1,
        qty: 1,
    }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(r.sheet().items.len(), 1, "equipment is an item with identity");
    r.tick(&[Command::Buy {
        player: P1,
        index: 1,
        qty: 1,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::OutOfStock]);
    // After `restock_ticks` the piece is back.
    for _ in 0..51 {
        r.tick(&[]);
    }
    r.tick(&[Command::Buy {
        player: P1,
        index: 1,
        qty: 1,
    }]);
    assert_eq!(r.refusals(), vec![]);
}

#[test]
fn walking_away_closes_the_shop() {
    let mut r = Run::new();
    r.enter(100);
    let smith = r.entity_of::<crate::zone::NpcComp>();
    r.tick(&[Command::OpenShop {
        player: P1,
        target: smith,
    }]);
    assert!(matches!(r.out.shops.first(), Some((_, Some(_)))));
    r.tick(&[Command::Move {
        player: P1,
        destination: TilePos::new(8, 3),
    }]);
    for _ in 0..40 {
        r.tick(&[]);
        if matches!(r.out.shops.first(), Some((_, None))) {
            break;
        }
    }
    assert_eq!(r.out.shops, vec![(P1, None)], "the shop closed by itself");
    // Buying with no shop open is refused.
    r.tick(&[Command::Buy {
        player: P1,
        index: 0,
        qty: 1,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::NoShop]);
}

#[test]
fn a_weapon_wears_out_breaks_and_can_be_repaired() {
    let mut r = Run::new();
    r.enter(10_000);
    let smith = r.entity_of::<crate::zone::NpcComp>();
    r.tick(&[Command::OpenShop {
        player: P1,
        target: smith,
    }]);
    r.tick(&[Command::Buy {
        player: P1,
        index: 1,
        qty: 1,
    }]);
    let slot = slot_of(&r, "sword");
    r.tick(&[Command::Equip { player: P1, slot }]);
    assert_eq!(durability(&r), Some((40, 40)));

    // Walk up to the dummy and keep hitting it: each blow that lands
    // costs one point of durability.
    r.tick(&[Command::Move {
        player: P1,
        destination: TilePos::new(5, 1),
    }]);
    for _ in 0..30 {
        r.tick(&[]);
    }
    let dummy = r.entity_of::<crate::fight::MonsterComp>();
    r.tick(&[Command::Attack {
        player: P1,
        target: dummy,
    }]);
    for _ in 0..400 {
        r.tick(&[]);
        if durability(&r).is_some_and(|(cur, _)| cur <= 30) {
            break;
        }
    }
    let worn = durability(&r).unwrap();
    assert!(worn.0 <= 30, "the sword wore out with use: {worn:?}");

    // Back to the smith: a repair costs coins and a little maximum.
    r.tick(&[Command::Stop { player: P1 }]);
    r.tick(&[Command::Move {
        player: P1,
        destination: TilePos::new(3, 1),
    }]);
    for _ in 0..40 {
        r.tick(&[]);
    }
    r.tick(&[Command::Unequip {
        player: P1,
        slot: EquipSlot::Weapon,
    }]);
    r.tick(&[Command::OpenShop {
        player: P1,
        target: smith,
    }]);
    let slot = slot_of(&r, "sword");
    let before = r.sheet().coins;
    r.tick(&[Command::Repair { player: P1, slot }]);
    assert_eq!(r.refusals(), vec![]);
    assert!(r.sheet().coins < before, "the repair is a coin sink");
    assert_eq!(
        r.out.coins.iter().filter(|(f, _)| *f == CoinFlow::Repair).count(),
        1
    );
    let (cur, max) = bag_durability(&r, slot).unwrap();
    assert_eq!(cur, max, "repaired to the full maximum");
    assert!(max < 40, "an NPC repair costs maximum durability: {max}");

    // A whole piece needs no repair.
    r.tick(&[Command::Repair { player: P1, slot }]);
    assert_eq!(r.refusals(), vec![Refusal::NotTraded]);
}

#[test]
fn a_piece_at_zero_breaks_and_leaves_the_hand() {
    let mut r = Run::new();
    r.enter(10_000);
    let smith = r.entity_of::<crate::zone::NpcComp>();
    r.tick(&[Command::OpenShop {
        player: P1,
        target: smith,
    }]);
    r.tick(&[Command::Buy {
        player: P1,
        index: 1,
        qty: 1,
    }]);
    let slot = slot_of(&r, "sword");
    r.tick(&[Command::Equip { player: P1, slot }]);
    // One blow from breaking.
    let h = *r.zone.by_player.get(&P1).unwrap();
    if let Ok(gear) = r.zone.world.query_one_mut::<&mut crate::gear::Gear>(h)
        && let Some(inst) = gear.equipped.iter_mut().find(|(s, _)| *s == EquipSlot::Weapon)
    {
        inst.1.durability = 1;
    }
    r.tick(&[Command::Move {
        player: P1,
        destination: TilePos::new(5, 1),
    }]);
    for _ in 0..30 {
        r.tick(&[]);
    }
    let dummy = r.entity_of::<crate::fight::MonsterComp>();
    r.tick(&[Command::Attack {
        player: P1,
        target: dummy,
    }]);
    let mut broke = false;
    for _ in 0..200 {
        r.tick(&[]);
        if durability(&r).is_none() {
            broke = true;
            break;
        }
    }
    assert!(broke, "at zero the piece is destroyed");
    assert!(r.sheet().items.is_empty(), "a broken piece is gone for good");
}

#[test]
fn gathering_empties_the_node_for_that_player_only() {
    let mut r = Run::new();
    r.enter(0);
    let vein = r.entity_of::<crate::craft::NodeComp>();
    // Out of reach from the spawn.
    r.tick(&[Command::Gather {
        player: P1,
        target: vein,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::TooFar]);
    r.tick(&[Command::Move {
        player: P1,
        destination: TilePos::new(3, 1),
    }]);
    for _ in 0..20 {
        r.tick(&[]);
    }
    r.tick(&[Command::Gather {
        player: P1,
        target: vein,
    }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(r.stack("ingot"), 2, "the table always gives two");
    assert!(
        r.sheet()
            .progress
            .proficiencies
            .iter()
            .any(|(p, _, xp)| p == "mining" && *xp > 0),
        "gathering raises the proficiency by use"
    );
    // Empty for this player until the cooldown runs out.
    r.tick(&[Command::Gather {
        player: P1,
        target: vein,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::NothingToGather]);
    for _ in 0..31 {
        r.tick(&[]);
    }
    r.tick(&[Command::Gather {
        player: P1,
        target: vein,
    }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(r.stack("ingot"), 4);
}

#[test]
fn crafting_needs_the_recipe_and_signs_the_piece() {
    let mut r = Run::new();
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.backpack = vec![Some(crate::types::SavedStack {
        item: "ingot".into(),
        qty: 4,
    })];
    sheet.progress.class = Some("artisan".into());
    sheet.progress.level = 1;
    r.tick(&[
        Command::Enter {
            player: P1,
            sheet: Box::new(sheet),
            pos: Some(TilePos::new(1, 1)),
        },
        Command::ItemIds { start: 1, end: 100 },
    ]);
    let recipe = r.zone.content().catalog.recipe_by_name("sword").unwrap().0;
    // Not known yet.
    r.tick(&[Command::Craft { player: P1, recipe }]);
    assert_eq!(r.refusals(), vec![Refusal::CannotCraft]);

    // Learned (a dialogue teaches it in the real content).
    let h = *r.zone.by_player.get(&P1).unwrap();
    r.zone
        .world
        .query_one_mut::<&mut crate::zone::PlayerComp>(h)
        .unwrap()
        .data
        .learn(recipe);
    r.tick(&[Command::Craft { player: P1, recipe }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(r.stack("ingot"), 2, "the materials were spent");
    let items = r.sheet().items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].crafter.as_deref(), Some("hero"), "the piece is signed");
    assert!(items[0].rarity.is_some_and(|x| x >= Rarity::Common));
    assert!(r.sheet().progress.xp > 0, "a craft gives level XP");
    assert_eq!(r.sheet().recipes, vec!["sword".to_owned()], "the recipe is saved");

    // Out of materials.
    r.tick(&[Command::Craft { player: P1, recipe }]);
    r.tick(&[Command::Craft { player: P1, recipe }]);
    assert_eq!(r.refusals(), vec![Refusal::CannotCraft]);
}

#[test]
fn a_craft_never_passes_the_class_cap() {
    let cat = content();
    let mut rng = base::Rng64::new(7);
    // A class capped at Epic, with the best proficiency and DEX: never
    // past Epic, however many rolls.
    for _ in 0..200 {
        let r = crate::craft::roll_quality(Rarity::Common, Rarity::Epic, 100, 100, &cat.catalog, &mut rng);
        assert!(r <= Rarity::Epic, "{r:?}");
    }
    // Without a production class the cap is Common.
    for _ in 0..50 {
        let r = crate::craft::roll_quality(Rarity::Common, Rarity::Common, 100, 100, &cat.catalog, &mut rng);
        assert_eq!(r, Rarity::Common);
    }
}

/// The backpack slot holding `item`.
fn slot_of(r: &Run, item: &str) -> u16 {
    let sheet = r.sheet();
    let stack = sheet
        .backpack
        .iter()
        .position(|s| s.as_ref().is_some_and(|s| s.item == item));
    if let Some(i) = stack {
        return i as u16;
    }
    // Equipment sits in the backpack through `Gear`.
    let h = *r.zone.by_player.get(&P1).unwrap();
    let gear = r.zone.world.get::<&crate::gear::Gear>(h).unwrap();
    let cat = &r.zone.content().catalog;
    gear.bag
        .iter()
        .find(|(_, i)| cat.item(i.item).is_some_and(|d| d.id == item))
        .map(|(s, _)| *s)
        .unwrap()
}

/// Durability of the piece in a backpack slot.
fn bag_durability(r: &Run, slot: u16) -> Option<(u32, u32)> {
    let h = *r.zone.by_player.get(&P1).unwrap();
    let gear = r.zone.world.get::<&crate::gear::Gear>(h).unwrap();
    let (_, i) = gear.bag.iter().find(|(s, _)| *s == slot)?;
    Some((i.durability, i.max_durability))
}

fn durability(r: &Run) -> Option<(u32, u32)> {
    let h = *r.zone.by_player.get(&P1).unwrap();
    let gear = r.zone.world.get::<&crate::gear::Gear>(h).unwrap();
    let i = gear.slot(EquipSlot::Weapon)?;
    Some((i.durability, i.max_durability))
}

/// Rarity adds damage, durability and value; from Unique up a piece may
/// carry a power; a production class's mastery adds durability.
#[test]
fn rarity_mastery_and_powers_shape_a_piece() {
    use crate::gear::{Gear, ItemInstance};
    let content = content();
    let cat = &content.catalog;
    let mut ids = base::IdBlock::new(1, 100);
    let mut rng = base::Rng64::new(3);
    let relic = cat.item_by_name("relic").unwrap().0;
    let inst = ItemInstance::create(relic, cat, &mut ids, &mut rng).unwrap();
    // Mythic: +150% durability, +60% damage, its one power always.
    assert_eq!((inst.durability, inst.max_durability), (100, 100));
    assert_eq!(inst.power_permille(cat), 600);
    assert_eq!(
        inst.power,
        Some(base::ItemPower::Attribute(base::Attribute::Dex, 3))
    );
    let mut gear = Gear::default();
    gear.put_equipped(EquipSlot::Weapon, inst);
    assert_eq!(gear.bonus().dex, 3, "the power counts while worn");
    assert_eq!(gear.weapon_permille(cat), 600);

    // A rare sword with mastery: +25% durability, then the mastery on top.
    let sword = cat.item_by_name("sword").unwrap().0;
    let mut s = ItemInstance::create(sword, cat, &mut ids, &mut rng).unwrap();
    assert_eq!(s.max_durability, 40);
    s.rarity = Rarity::Rare;
    s.mastery = 1000;
    s.shape(cat, &mut rng);
    assert_eq!(s.max_durability, 55, "40 x 1.25 x 1.10");
    assert_eq!(s.power, None, "a rare piece has no power");

    // Mastery: almost nothing for a new artisan, capped at 10%.
    let e = &cat.formulas().economy;
    assert_eq!(e.mastery(1, 5, 1), 1, "0.01%");
    assert_eq!(e.mastery(40, 30, 1), 240);
    assert_eq!(e.mastery(100, 100, 2), 1000);
    assert_eq!(e.mastery(100, 100, 0), 0, "no production class, no mastery");
}

/// A rare piece sells for twice a common one; a mythic one is not bought.
#[test]
fn a_rare_piece_sells_for_more() {
    let price = |rarity: Rarity| {
        let mut r = Run::new();
        let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
        sheet.items.push(crate::types::SavedItem {
            id: 5,
            item: "sword".into(),
            place: crate::types::ItemPlace::Backpack(0),
            hidden: Vec::new(),
            appraised: false,
            durability: (0, 0),
            crafter: None,
            rarity: Some(rarity),
            mastery: 0,
            power: None,
        });
        r.tick(&[Command::Enter {
            player: P1,
            sheet: Box::new(sheet),
            pos: Some(TilePos::new(1, 1)),
        }]);
        let smith = r.entity_of::<crate::zone::NpcComp>();
        r.tick(&[Command::OpenShop {
            player: P1,
            target: smith,
        }]);
        let before = r.sheet().coins;
        r.tick(&[Command::Sell {
            player: P1,
            slot: 0,
            qty: 1,
        }]);
        (r.sheet().coins - before, r.refusals())
    };
    let (common, _) = price(Rarity::Common);
    let (rare, _) = price(Rarity::Rare);
    assert!(common > 0);
    assert_eq!(rare, common * 2);
    // Epic and up go to players only (`npc_buy_max`).
    for rarity in [Rarity::Epic, Rarity::Mythic] {
        assert_eq!(price(rarity), (0, vec![Refusal::NotTraded]), "{rarity:?}");
    }
}

/// Enters a character without a class at `pos`, with materials.
fn enter_plain(r: &mut Run, pos: TilePos) {
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.backpack = vec![Some(crate::types::SavedStack {
        item: "ingot".into(),
        qty: 4,
    })];
    sheet.recipes = vec!["sword".into()];
    sheet.coins = 2_000;
    r.tick(&[
        Command::Enter {
            player: P1,
            sheet: Box::new(sheet),
            pos: Some(pos),
        },
        Command::ItemIds { start: 1, end: 100 },
    ]);
}

/// Without a production class, crafting happens only in a workshop
#[test]
fn outside_a_production_class_crafting_needs_a_workshop() {
    let mut r = Run::new();
    enter_plain(&mut r, TilePos::new(1, 1));
    let recipe = r.zone.content().catalog.recipe_by_name("sword").unwrap().0;
    r.tick(&[Command::Craft { player: P1, recipe }]);
    assert_eq!(r.refusals(), vec![Refusal::CannotCraft]);
    assert_eq!(r.stack("ingot"), 4, "nothing was spent");

    let mut r = Run::new();
    enter_plain(&mut r, TilePos::new(2, 3));
    r.tick(&[Command::Craft { player: P1, recipe }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(r.stack("ingot"), 2);
}

/// Without a production class a piece is appraised at a shop, for coins,
/// and always revealed.
#[test]
fn a_shop_appraises_for_coins() {
    let mut r = Run::new();
    enter_plain(&mut r, TilePos::new(1, 1));
    let smith = r.entity_of::<crate::zone::NpcComp>();
    r.tick(&[Command::OpenShop {
        player: P1,
        target: smith,
    }]);
    let view = r.out.shops.first().cloned().unwrap().1.unwrap();
    assert_eq!(view.appraise_permille, 100);
    r.tick(&[Command::Buy {
        player: P1,
        index: 1,
        qty: 1,
    }]);
    let slot = slot_of(&r, "sword");
    r.tick(&[Command::Appraise { player: P1, slot }]);
    assert_eq!(r.refusals(), vec![Refusal::RequirementsNotMet]);
    let before = r.sheet().coins;
    r.tick(&[Command::ShopAppraise { player: P1, slot }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(r.sheet().coins, before - 100, "10% of the value");
    assert_eq!(r.out.coins, vec![(CoinFlow::Appraise, 100)]);
    assert!(r.sheet().items[0].appraised);
    r.tick(&[Command::ShopAppraise { player: P1, slot }]);
    assert_eq!(r.refusals(), vec![Refusal::NotReady], "once is enough");
}

/// A shop pays each player up to its purse per period.
#[test]
fn a_purse_runs_out_and_refills() {
    use crate::shop::{purse_left, spend_purse};
    let purse = defs::Purse {
        coins: 100,
        refill_secs: 60,
    };
    let mut sales = Vec::new();
    assert_eq!(purse_left(&sales, "forge", purse, 1_000), (100, 60));
    spend_purse(&mut sales, "forge", purse, 1_000, 70);
    assert_eq!(purse_left(&sales, "forge", purse, 1_010), (30, 50));
    assert_eq!(purse_left(&sales, "store", purse, 1_010), (100, 60), "per shop");
    spend_purse(&mut sales, "forge", purse, 1_020, 30);
    assert_eq!(purse_left(&sales, "forge", purse, 1_020).0, 0);
    // The period ended: full again, and the next sale opens a new one.
    assert_eq!(purse_left(&sales, "forge", purse, 1_060).0, 100);
    spend_purse(&mut sales, "forge", purse, 1_061, 10);
    assert_eq!(sales, vec![("forge".to_owned(), 10, 1_061)]);
}

#[test]
fn selling_counts_against_the_purse() {
    let mut r = Run::new();
    r.enter(3_000);
    let smith = r.entity_of::<crate::zone::NpcComp>();
    r.tick(&[Command::OpenShop {
        player: P1,
        target: smith,
    }]);
    r.tick(&[Command::Buy {
        player: P1,
        index: 0,
        qty: 5,
    }]);
    r.tick(&[Command::Sell {
        player: P1,
        slot: slot_of(&r, "bread"),
        qty: 5,
    }]);
    assert_eq!(r.sheet().shop_sales, vec![("forge".to_owned(), 15, 1_000)]);
    let view = r.out.shops.last().cloned().unwrap().1.unwrap();
    assert_eq!(view.purse, Some((100_000 - 15, 3_600)));
}

/// A monster with reach steps back from a player next to it and still
/// hits from afar (the goblin shaman).
#[test]
fn a_caster_keeps_its_distance() {
    let content = content_with(
        "(level: 1, hp: 100000, damage: (1, 1), xp: 1, aggro_radius: 6, leash: 30, range: 5, respawn_ticks: 5)",
    );
    let mut r = Run {
        zone: Zone::new(content, ZoneId::new(0), 11).unwrap(),
        out: TickOutput::default(),
    };
    enter_plain(&mut r, TilePos::new(5, 2));
    let caster = r.entity_of::<crate::fight::MonsterComp>();
    let mut blows_from_afar = 0;
    for _ in 0..80 {
        r.tick(&[]);
        let (_, at) = r.zone.body_of(caster).unwrap();
        let (_, me) = r.zone.body_of(r.sheet_id()).unwrap();
        blows_from_afar += r
            .out
            .combat
            .iter()
            .filter(|c| c.attacker == caster && at.distance(me) > 1)
            .count();
    }
    let (_, at) = r.zone.body_of(caster).unwrap();
    assert!(at.distance(TilePos::new(5, 2)) > 1, "stepped away to {at:?}");
    assert!(blows_from_afar > 0, "hits from afar");
}

/// A weapon that costs vigor takes it each blow; without it the blow waits
/// twice as long.
#[test]
fn a_heavy_blow_costs_vigor() {
    let mut f = crate::fight::Fighter::new(100, 10, 1);
    assert_eq!(crate::fight::blow_wait(&mut f, 20, 6), 20);
    assert_eq!(f.vigor, 4);
    assert_eq!(crate::fight::blow_wait(&mut f, 20, 6), 40);
    assert_eq!(f.vigor, 4, "nothing taken without enough");
    assert_eq!(crate::fight::blow_wait(&mut f, 20, 0), 20);
}

/// The last System message key and its arguments.
fn last_system(r: &Run) -> Option<(String, Vec<base::Arg>)> {
    let (_, t) = r.out.system.last()?;
    let key = r.zone.content().catalog.keys().key(t.id)?.to_owned();
    Some((key, t.args.to_vec()))
}

/// "not enough coins" says the price and what the player holds.
#[test]
fn short_of_coins_says_the_price() {
    let mut r = Run::new();
    r.enter(300);
    let smith = r.entity_of::<crate::zone::NpcComp>();
    r.tick(&[Command::OpenShop {
        player: P1,
        target: smith,
    }]);
    r.tick(&[Command::Buy {
        player: P1,
        index: 1,
        qty: 1,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::NotEnoughCoins]);
    let (key, args) = last_system(&r).unwrap();
    assert_eq!(key, "system.not_enough_coins");
    assert!(matches!(args[..], [base::Arg::Int(price), base::Arg::Int(300)] if price > 300));
}

/// A missing material is named, with what the recipe needs.
#[test]
fn a_missing_material_is_named() {
    let mut r = Run::new();
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.backpack = vec![Some(crate::types::SavedStack {
        item: "ingot".into(),
        qty: 1,
    })];
    sheet.recipes = vec!["sword".into()];
    sheet.progress.class = Some("artisan".into());
    sheet.progress.level = 1;
    r.tick(&[
        Command::Enter {
            player: P1,
            sheet: Box::new(sheet),
            pos: Some(TilePos::new(1, 1)),
        },
        Command::ItemIds { start: 1, end: 100 },
    ]);
    let recipe = r.zone.content().catalog.recipe_by_name("sword").unwrap().0;
    r.tick(&[Command::Craft { player: P1, recipe }]);
    assert_eq!(r.refusals(), vec![Refusal::CannotCraft]);
    let (key, args) = last_system(&r).unwrap();
    assert_eq!(key, "system.craft_material");
    assert!(
        matches!(args[1..], [base::Arg::Int(2), base::Arg::Int(1)]),
        "{args:?}"
    );
}

/// A production class mends its own piece with the recipe's
/// main material, in proportion to the wear; others cannot.
#[test]
fn a_production_class_mends_with_materials() {
    let mut r = Run::new();
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.backpack = vec![Some(crate::types::SavedStack {
        item: "ingot".into(),
        qty: 3,
    })];
    sheet.recipes = vec!["sword".into()];
    sheet.progress.class = Some("artisan".into());
    sheet.progress.level = 1;
    r.tick(&[
        Command::Enter {
            player: P1,
            sheet: Box::new(sheet),
            pos: Some(TilePos::new(1, 1)),
        },
        Command::ItemIds { start: 1, end: 100 },
    ]);
    let recipe = r.zone.content().catalog.recipe_by_name("sword").unwrap().0;
    r.tick(&[Command::Craft { player: P1, recipe }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(r.stack("ingot"), 1);
    let slot = slot_of(&r, "sword");
    // Whole: nothing to mend.
    r.tick(&[Command::SelfRepair { player: P1, slot }]);
    assert_eq!(r.refusals(), vec![Refusal::NotTraded]);
    // Worn to a quarter: 2 ingots × 3/4, rounded up, is 2; only 1 held.
    let h = *r.zone.by_player.get(&P1).unwrap();
    let set = |r: &mut Run, cur: u32| {
        let mut g = r.zone.world.get::<&mut crate::gear::Gear>(h).unwrap();
        let i = g.bag_item(slot).unwrap();
        i.durability = cur;
    };
    let max = bag_durability(&r, slot).unwrap().1;
    set(&mut r, max / 4);
    r.tick(&[Command::SelfRepair { player: P1, slot }]);
    assert_eq!(r.refusals(), vec![Refusal::RequirementsNotMet]);
    // Worn by a quarter: one ingot.
    set(&mut r, max - max / 4);
    r.tick(&[Command::SelfRepair { player: P1, slot }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(r.stack("ingot"), 0, "one ingot spent");
    let (cur, new_max) = bag_durability(&r, slot).unwrap();
    assert_eq!((cur, new_max), (max, max), "an artificer keeps the maximum");
    assert_eq!(r.sheet().coins, 0, "no coins asked");

    // Without a production class: refused.
    let mut r = Run::new();
    r.enter(10_000);
    let smith = r.entity_of::<crate::zone::NpcComp>();
    r.tick(&[Command::OpenShop {
        player: P1,
        target: smith,
    }]);
    r.tick(&[Command::Buy {
        player: P1,
        index: 1,
        qty: 1,
    }]);
    let slot = slot_of(&r, "sword");
    let h = *r.zone.by_player.get(&P1).unwrap();
    r.zone
        .world
        .get::<&mut crate::gear::Gear>(h)
        .unwrap()
        .bag_item(slot)
        .unwrap()
        .durability = 1;
    r.tick(&[Command::SelfRepair { player: P1, slot }]);
    assert_eq!(r.refusals(), vec![Refusal::RequirementsNotMet]);
    assert_eq!(last_system(&r).unwrap().0, "system.self_repair_class_only");
}

/// A buff technique raises the caster's defense for a while.
#[test]
fn the_vow_raises_defense_for_a_while() {
    let mut r = Run::new();
    let mut sheet = CharacterSheet::new(CharacterName::try_from("hero").unwrap());
    sheet.progress.techniques = vec![("vow".into(), 1, 0)];
    r.tick(&[Command::Enter {
        player: P1,
        sheet: Box::new(sheet),
        pos: Some(TilePos::new(1, 1)),
    }]);
    let h = *r.zone.by_player.get(&P1).unwrap();
    let defense = |r: &Run| {
        r.zone
            .world
            .get::<&crate::fight::Fighter>(h)
            .unwrap()
            .ward
            .defense(10, r.zone.tick.0)
    };
    assert_eq!(defense(&r), 10);
    let vow = r.zone.content().catalog.technique_by_name("vow").unwrap().0;
    let me = r.sheet_id();
    r.tick(&[Command::UseTechnique {
        player: P1,
        technique: vow,
        target: me,
    }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(defense(&r), 15, "+50%");
    assert_eq!(
        r.zone
            .world
            .get::<&crate::fight::Fighter>(h)
            .unwrap()
            .ward
            .defense(0, r.zone.tick.0),
        4,
        "at least the flat points"
    );
    for _ in 0..101 {
        r.tick(&[]);
    }
    assert_eq!(defense(&r), 10, "it ran out");
}
