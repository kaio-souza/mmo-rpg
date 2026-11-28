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

- **`portals`**: `[(min: (x: 10, y: 4), max: (x: 10, y: 4), to: (x: 300, y: 12))]`. Stepping on a tile of the rectangle takes the player to `to`, a tile of any zone (global coordinates): a jump in the same zone, or a hand-off to the zone whose map holds it, like crossing the seam. `to` must be walkable and never on another portal. An NPC does the same with the dialogue effect `Teleport((x, y))`.
- **`instance`**: `Some((exit: (x: 18, y: 15), close_after_secs: 60, time_limit_secs: 0))` makes the zone a dungeon ([dungeons](../dungeons-and-raids.md)). It is never one shared zone: a portal or `Teleport` aimed at it opens a **copy** for the player's group (its leader at that moment), with its own monsters and seed; the rest of the group lands in the same copy. An empty copy closes after `close_after_secs`; with `time_limit_secs`, everyone inside goes to `exit` when it runs out; a character who leaves the game inside, or falls there, comes back at `exit`. An instance touches no other zone, is never the starting zone, and its `exit` is walkable ground of the open world. Inside, portals lead back out.

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

Optional: `range` (attacks from a distance and keeps it), `weapon_prof`, `loot_table` (a shared [loot table](items-and-economy.md#loot)), `phases` (a boss, below), `tame` (below). The name is the text key `monster.<id>.name`. Threat decides who a monster attacks; XP is shared by everyone who did at least 10% of the threat.

### Boss phases

```ron
phases: [
    (below_hp_permille: 600, damage_permille: 1200, area: 1),
    (below_hp_permille: 250, damage_permille: 1500, attack_ticks: Some(45),
     status: Some((kind: Stun, duration_ticks: 20))),
],
```

From a share of HP down (thousandths, each phase lower than the one before) the monster hits as the phase says: damage, pace, `area` (every player within that many tiles of its target) and a [status](characters.md#techniques). The deepest phase reached holds until the monster resets; the players fighting it hear `system.boss_phase`.

### Taming

```ron
tame: Some((
    requires: [Proficiency("taming", 10), Item(item: "raw_meat", qty: 1)],
    below_hp_permille: 300,   // weakened to 30 % of its HP or less (1000: any)
    chance_permille: 500,     // of each attempt (1000: always)
)),
```

A monster with `tame` can become a player's [companion](../mounts-and-companions.md): the player right clicks it and chooses **Tame**. `requires` is open, all must hold, so each game picks its own rule: `Level(n)`, `Attribute(Str, n)`, `Proficiency("id", n)` (a taming skill or a profession), `Title("id")`, `Class("id")`, `Karma(n)` (negative: at most) and `Item(item, qty)` (a bait, taken on every attempt). Without `tame` it cannot be tamed. A tamed monster leaves the world as if defeated and respawns later; the tamer counts a `tames` [counter](characters.md#counters) keyed by monster.

### Weather

```ron
weather: [(kind: Rain, permille: 150), (kind: Fog, permille: 80)],   // in zone.ron
```

Each world hour a zone may rain, snow or be foggy by these chances (they add up to 1000 at most; the rest is clear). Every player of the zone sees the same, computed from the zone and the hour, on the server too, so it can drive rules as well as pictures:

- **Pictures**: rain, snow or fog over the screen; tiles may have a `rain_sprite` and a `snow_sprite` ([assets](../assets.md#floors-tilesron)), like the `day_sprite` of day and night.
- **Rules**: the dialogue condition `Moment(Rain)` ([dialogues](npcs-and-quests.md)) and a monster spawn's `when` (below). A `Moment` is `Day`, `Night`, `Rain`, `Snow`, `Fog` or `Clear`; night for the rules runs from `night_from_permille` to `night_until_permille` of the world day (`progression/formulas.ron`, 800 and 250).

A spawn in `monsters.ron` may be only for a moment: `(monster: "ghost", pos: (x: 10, y: 4), when: Some(Night))`. It comes when the moment starts and leaves a few seconds after it ends, unless it is fighting. The map page sets it per spawn, and the zone's weather chances in its Weather card.

## Dungeons

`dungeons/<id>.ron` gives an [instance](#zoneron) zone its rules; the name is `dungeon.<id>.name`.

```ron
(zone: "crypt", party: (1, 6), level: (5, 30), boss: "crypt_lord", lockout: Daily)
```

The door (a portal or `Teleport` into the zone) asks for the level range and a group of `party` size (alone is 1) and says why when it refuses. Killing `boss` clears the run: everyone inside counts a `clears` with the dungeon as key (titles can use it), the time goes to the fastest clears (`RankBy::FastestClear("crypt")` in a [ranking](characters.md#rankings)), the first party of the server is told to everyone, and with `lockout: Daily` or `Weekly` they cannot enter again until the next midnight or Monday (UTC).

## Day and night

The world clock comes from `day_secs` in `progression/formulas.ron` (default 7200: a day every two real hours). Today night only changes the picture (tint, lights); `assets/light.ron` sets the colours ([assets](../assets.md#light)).
