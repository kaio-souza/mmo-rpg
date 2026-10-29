# Items and economy

Examples to copy: [`templates/new-game/content/items/`](../../templates/new-game/content/items), [`examples/riftara/content/`](../../examples/riftara/content) (`items/`, `shops/`, `recipes/`, `nodes/`, `loot/`).

## Items

```ron
// a consumable
(kind: Consumable, max_stack: 15, weight: 120, rarity: Common, value: 300, durability: 0,
 use: Some((hp: 100, cooldown_ticks: 400)), icon: "health_potion")

// a weapon
(kind: Weapon, weight: 2500, rarity: Common, value: 400, durability: 80,
 equip: Some(Weapon(damage: (3, 7), attack_ticks: 30, attribute: Str,
                    proficiency: "sword", visual: "rusty_sword", vigor: 3)),
 hidden: [(attribute: Str, min: 0, max: 3)], icon: "rusty_sword")
```

| Field | Meaning |
|---|---|
| `kind` | `Material`, `Consumable`, `Quest`, `Weapon`, `Armor`, `Offhand` |
| `max_stack`, `weight`, `value` | stack size, weight, base price in copper |
| `binding` | `Free` (default), `Character`, `Account` |
| `rarity` | `Common`, `Rare`, `Epic`, `Unique`, `Legendary`, `Mythic`; crafted pieces roll it |
| `durability` | 0 for items that do not wear; equipment loses it with use and breaks at zero |
| `use` | `hp`, `vigor`, `mp`, `revive_permille` (revives a fallen ally), `cooldown_ticks` |
| `equip` | `Weapon(damage, attack_ticks, attribute, proficiency, range, visual, vigor)`, `Armor(defense, visual)`, `Offhand(defense, visual)` |
| `hidden` | attribute bonuses rolled on each piece, revealed by appraisal |
| `light` | `Some((radius: 4))`: equipped, it lights the wearer (picture only); `burn` wears it per minute equipped |
| `icon` | icon name; `visual` names the layer drawn over the body |

Every non-stackable item has its own identity, stored with exactly one owner; items can never be copied ([security](../security.md)).

## Loot

Monsters list their own drops (`loot: [(item, chance, qty)]`). A shared table, `loot/<id>.ron`, holds what several monsters or nodes drop: `(coins: (1, 4), entries: [(item: "iron_ore", chance: 800)])`.

## Shops

```ron
(
    repairs: false, appraises: false,
    purse: Some((coins: 4000, refill_secs: 3600)),   // what the shop pays each player per period
    stock: [
        (item: "travel_bread", qty: 0),                       // 0: unlimited
        (item: "sunroot", qty: 15, restock_ticks: 1200),
    ],
)
```

`price_permille` changes this shop's prices. An NPC opens a shop with `shop: Some("<id>")`. Shops buy from players only up to `npc_buy_max` rarity.

## Gathering and crafting

```ron
// nodes/iron_vein.ron: a resource node; it empties for one player, not for all
(sprite: "iron_vein", proficiency: "mining", cooldown_ticks: 600, loot: "iron_vein", xp: 12)

// recipes/cloth_tunic.ron
(proficiency: "smithing", min_points: 1, materials: [(item: "wolf_pelt", qty: 6)],
 result: "cloth_tunic", xp: 30)
```

Recipes are taught by NPC dialogue (`LearnRecipe`). Crafted equipment is signed with its maker and rolls rarity up to the maker's class `quality_cap`. Players without a production class craft only inside a `Workshop` region and earn almost no crafting XP.

## Economy numbers

`economy/formulas.ron` (all optional): selling and buying price (`sell_permille`, `buy_permille`), repair (`repair_permille`, `repair_max_loss_permille`), `appraise_permille`, durability loss (`durability_per_hit`, `death_durability_permille`, `warn_durability_permille`), crafting (`craft_xp_permille`, `craft_quality_*`), `gather_cooldown_ticks`, per-rarity tables (`rarity_power_permille`, `rarity_durability_permille`, `rarity_value_permille`, `power_chance_permille`, `hidden_chance_permille`), `npc_buy_max`, and the maker's mastery (`mastery_divisor`, `mastery_max`).

Money leaves the world on purchases and repairs; the server records coins created and destroyed per source, and `admin report economy` shows the balance.
