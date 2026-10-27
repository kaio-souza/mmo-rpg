# Concepts

## Game folder

A game is a folder:

```text
mygame/
├── content/   the rules, as RON files (items, monsters, zones, quests, texts...)
├── assets/    what the client shows and plays (images, fonts, themes, sounds, interface texts)
├── data/      created by the server: database, TLS certificate, backups
└── replays/   created by the server: zone recordings after a crash
```

`server`, `client`, `editor`, `admin` and `validate` read `content/` and `assets/` relative to where they run; `--content` and `--assets` point elsewhere.

## Content is data

- Every rule a game designer would change is a file in `content/`: what a sword does, what a wolf drops, which dialogue option appears, what earns a title.
- There is **no scripting language**. Behaviour is built from closed blocks the engine knows (conditions, effects, objectives, rewards, bonuses). A new kind of block is engine work; using it is content work.
- Content is **validated before it loads** (`validate`, the editor, and the server on every reload). Invalid content never runs.
- Content ids are stable text (`"wolf_pelt"`). Saved characters store those names, so an id is never renamed or deleted once players have it; content only grows.

Details: [content](content/README.md).

## The isometric world

| Thing | Size |
|---|---|
| Tile | 64×32 px diamond |
| Character frame | 64×64 px, feet at pixel (32, 58) |
| Tall objects and walls | 64×72 px, base at pixel (32, 56) |
| Icon | 32×32 px |

Projection, with tile width `W` = 64 and height `H` = 32:

```text
screen_x = (tile_x − tile_y) × W/2 − camera_x
screen_y = (tile_x + tile_y) × H/2 − camera_y
```

- **Drawing order:** floor first (pre-rendered per chunk), then objects, creatures and walls sorted by `tile_x + tile_y`, then effects, light and the interface.
- **Directions:** four are drawn, in this row order in a sprite strip: south, west, north, east. A diagonal shows the previous direction clockwise (south-east draws south).
- **Movement:** tile by tile; one creature per tile. The server says "from A to B in N ticks" and the client animates in between.
- **Anchor:** the pixel of an image that sits on the centre of its tile. See [assets](assets.md).

## Time

- The server runs **20 ticks per second** (50 ms). Rules count time in ticks (`attack_ticks: 30` is 1.5 s).
- Long waits that must run while a player is offline (bans, long cooldowns) are stored as absolute time.
- A world clock gives day and night; its length is content.

## Authoritative server

The client only shows what the server says and sends **intentions** ("walk to this tile", "use this technique on that target"). The server checks and decides. The client never computes damage or validates movement.

## Zones and the open world

- The world is cut into **zones**, each a map with its own simulation, placed side by side by their `origin`.
- Where two zones touch there is a **seam**: each zone also simulates a 24-tile band of its neighbour, so players see across, and walking across hands the player over without a loading screen.
- Zones share no state, so moving a zone to another process or machine is configuration ([runtime](runtime.md)).

## Regions

Rectangles inside a zone with a rule: `Safe` (no combat), `Pvp` (players may attack players) and `Workshop` (crafting allowed without a production class). Everywhere else players fight monsters only.

## What a player sees

Each player receives only what is within the zone's `view_radius` and in line of sight. Hidden things are never sent, which stops map hacks at the root. Ground outside sight is drawn under fog.

## Texts

Content and messages carry **text keys**, never sentences. The client turns keys into text with the text pack of the player's language, downloaded once and cached by content hash. See [content/text](content/text.md).

## Determinism

The zone simulation uses no wall clock, no unseeded randomness and no floating point in rules. A recorded zone replays to the same state hash, tick by tick; the server writes a replay when a zone crashes ([runtime](runtime.md#crashes-and-replays)).
