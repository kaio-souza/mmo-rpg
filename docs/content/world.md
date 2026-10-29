# World: zones, maps and spawns

A zone is a folder `content/zones/<zone>/`. Example to copy: [`templates/new-game/content/zones/meadow/`](../../templates/new-game/content/zones/meadow).

## `zone.ron`

```ron
(
    origin: (x: 0, y: 0),        // global tile of the map's top-left corner
    view_radius: 20,             // how far players see, in tiles
    spawn: (x: 20, y: 15),       // where new characters appear
    spawn_radius: 2,             // they land on a free tile this close to `spawn`
    regions: [
        (kind: Safe, min: (x: 16, y: 12), max: (x: 24, y: 18)),
    ],
)
```

- **`origin`** places the zone in the world. Zones that touch by a side or a corner become seam neighbours ([concepts](../concepts.md#zones-and-the-open-world)); zones must not overlap.
- **`regions`** are rectangles in global coordinates, corners included: `Safe` (no combat), `Pvp` (players may attack players), `Workshop` (crafting without a production class). Where rectangles overlap, the last one wins; a workshop may sit inside another region.
- With more than one zone, exactly one has `starting: true`: new characters, and characters whose zone is gone, start there.

## `map.ron`

```ron
(
    legend: {
        '.': (floor: 1),
        '~': (floor: 4, blocks_movement: true),
        '#': (floor: 3, blocks_movement: true, blocks_sight: true),
    },
    rows: [
        "~~~~~~",
        "~....~",
        "~~~~~~",
    ],
)
```

- One character per tile, one string per row, all rows the same length. Characters are layout, never game text.
- `floor` is a number that `assets/tiles.ron` turns into an image ([assets](../assets.md#floors-tilesron)). What blocks walking or sight is a game rule and lives here; how a floor looks lives in `tiles.ron`.
- `switch: Some(<floor>)` makes a tile players can toggle for everyone (a brazier lit and unlit).
- The editor paints maps with brush, bucket, rectangle and line tools, with undo.

## Spawns

```ron
// monsters.ron
[ (monster: "wolf", pos: (x: 30, y: 7), count: 3, radius: 3) ]
// npcs.ron
[ (npc: "guide", pos: (x: 36, y: -31)) ]
// nodes.ron
[ (node: "iron_vein", pos: (x: 46, y: 16)) ]
```

Positions are global tiles. All three files are optional.

## Monsters

`content/monsters/<id>.ron`:

```ron
(
    level: 2, hp: 28, attributes: (str: 6, vit: 5, agi: 6, dex: 5),
    damage: (2, 4), defense: 1, attack_ticks: 30,
    aggro_radius: 4,      // attacks players this close
    leash: 14,            // gives up the chase this far from home
    respawn_ticks: 400, xp: 40,
    sprite: "wolf", icon: "wolf",
    coins: (2, 8),
    loot: [(item: "wolf_pelt", chance: 600)],   // optional qty: (1, 3)
)
```

Optional: `range` (attacks from a distance and keeps it), `weapon_prof`, `loot_table` (a shared [loot table](items-and-economy.md#loot)). The name is the text key `monster.<id>.name`. Threat decides who a monster attacks; XP is shared by everyone who did at least 10% of the threat.

## Day and night

The world clock comes from `day_secs` in `progression/formulas.ron` (default 7200: a day every two real hours). Today night only changes the picture (tint, lights); `assets/light.ron` sets the colours ([assets](../assets.md#light)).
