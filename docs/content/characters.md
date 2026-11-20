# Characters: attributes, classes, proficiencies, techniques, titles

Examples to copy: [`examples/riftara/content/`](../../examples/riftara/content) (`classes/`, `proficiencies/`, `techniques/`, `titles/`).

## Attributes and resources

Six attributes: `str`, `vit`, `agi`, `dex`, `int`, `spi`. Three resources: HP, vigor (spent by blows and techniques) and MP (maximum from INT, regeneration from SPI). Characters start with `start_attribute` in each and get `free_points` per level (`free_points_without_class` before choosing a class).

## Classes

```ron
(
    tier: 1, level: 5,                 // offered from level 5
    parent: None,                      // tier 2+: the class it grows from
    requires: [Any([Proficiency("sword", 5), Attribute(Str, 15)])],   // all must hold; Any = one of
    auto_points: (str: 2, vit: 1),     // attribute points given per level
    growth: [("sword", 1200)],         // proficiencies that grow faster (thousandths)
)
```

Optional: `specials` (special attributes the class grows per level: `(special: "faith", base: 5, per_level_permille: 500)`), `techniques` (taught when the class is chosen), `craft_xp_permille` and `quality_cap` (production classes), `icon`. Choosing a class is permanent. Tiers 1 to 4 work; the level of each tier is content.

## Proficiencies

`(kind: Weapon)` — kinds: `Weapon`, `Defense`, `Magic`, `Production`. A proficiency goes from 0 to 100 points and grows by use, with less gain for repeating the same thing. Weapons and nodes name the proficiency they train.

## Techniques

```ron
(proficiency: "bow", unlock_at: 10, cooldown_ticks: 80, vigor: 12,
 damage_permille: 1600, growth_permille: 70, icon: "")
```

Unlocked when the proficiency reaches `unlock_at`; levels 1 to 10 by use; usable only with a weapon of that proficiency in hand (support magic such as healing needs no weapon). Optional: `mp` (cost), `range`, `heal_permille`, `revive_permille`, `buff: Some((stat: Defense, permille: 250, flat: 3, duration_ticks: 600))`, `requires_special: [("faith", 5)]`, `taught_only: true` (only an NPC, a quest or a class teaches it).

## Titles

```ron
(
    trigger: Level(1),                         // what earns it
    bonuses: [PercentUntilLevel(Xp, 5, 10)],   // +5% XP until level 10
    techniques: [],                            // taught when earned
    replaces: [],                              // titles it takes away
    icon: "",
)
```

| Triggers | Bonuses (every owned title counts, always) |
|---|---|
| `Level(n)` | `Attribute(Str, n)` (on the attribute in use, never the base) |
| `Counter(family: "kills", key: "wolf", at: 100)` (empty key: family total) | `MaxHp(n)`, `MaxVigor(n)` |
| `Proficiency("sword", 50)` | `Percent(Damage | Xp | MaxMp, n)` |
| `Attribute(Str, 30)` (base value) | `PercentUntilLevel(Xp, 5, 10)` |
| | `Special("faith", n)` |

A title that `replaces` others removes them when earned, and they are never earned again while it is owned. Titles also serve as class requirements and show identity.

## Special attributes

`special_attributes/<id>.ron` declares an attribute from 0 to 100 earned by titles or grown by a class (faith, light). What it does is engine work, added the first time a game needs it; today it can gate techniques and add to a proficiency in use.

## Counters

`counters.ron` lists the event families counted per character, for titles: `["kills", "deaths"]`. `kills` and `quests` are keyed by monster and quest id.

## Appearance

`appearance/appearance.ron` lists bodies, skins, hair styles and hair colours offered at character creation, each with an id and a colour; names are text keys `appearance.<list>.<id>`. Append only: saved characters name these ids.

A character is drawn in layers, bottom first, from sprites in `assets/sprites/creatures/`:

| Layer | Field | Tinted by |
|---|---|---|
| body | `sprite` on a body | the skin's colour |
| clothes | `clothes` on a body | nothing: drawn as it is |
| hair | `sprite` on a hair style | the hair colour's colour |

```ron
bodies: [(id: "body_a", color: "#5b6b8c", sprite: "body_a", clothes: "clothes_a")],
hair_styles: [(id: "short", sprite: "hair_short"), (id: "bald")],
```

A tinted layer is drawn in light greys, so the colour shows true ([assets](../assets.md)). A body without `sprite` is drawn as the `player` sprite in the skin colour; a hair style without one draws no hair (bald). Skins and hair colours have no sprite. The creation screen previews exactly these layers and offers only the lists that change them: bodies when one has a sprite, hair styles and colours when a hair style has one.

## Formulas

`progression/formulas.ron` (all optional): `max_level`, `level_xp_base`, `level_xp_exponent`, `start_attribute`, `free_points`, `free_points_without_class`, `fallen_ticks` (time down before death), `newbie_level` (no death penalty up to it), `death_lock_secs` (character locked after dying), `day_secs`.

`combat/formulas.ron`: `hit_min_permille`, `hit_max_permille`, `crit_damage_permille`, `pvp_damage_permille`, `global_cooldown_ticks`.
