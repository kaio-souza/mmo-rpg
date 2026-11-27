# New game template

The starting point of a game made with the kit. Copy this folder and you have a playable game: one area (`meadow`) with a safe stone plaza that is also a workshop, dirt roads, a pond, mud and water all around. On the plaza, a merchant (store, smelting and sword lessons) and a herbalist (a quest for herbs, the health potion recipe). Around it: wolves, shadow wolves (a rare dark sword), a frost wolf whose bite slows, iron and silver veins to mine and herb patches to gather; ore smelts into ingots and ingots into swords. System, error and interface texts in ten languages; the title is "My Game".

## Start

```sh
cp -r templates/new-game ~/mygame
cd ~/mygame
editor                                            # build the game
server --dev                                      # another terminal
client --dev --server 127.0.0.1:7000 --name Ana   # another terminal
```

With the kit's binaries on your `PATH` (`cargo build --release` in the kit). Without it: `cargo run --release --manifest-path ~/mmo-kit/Cargo.toml -p editor`, and the same for `server` and `client`. See [getting started](../../docs/getting-started.md).

## What is here

| Folder | What |
|---|---|
| `content/zones/meadow/` | the area: `zone.ron` (spawn, safe region), `map.ron` (the map), `monsters.ron` (wolves) |
| `content/monsters/`, `content/items/` | the wolf and what it drops |
| `content/combat/`, `content/progression/`, `content/economy/` | balance numbers (hit chances, level curve, death, prices, durability, crafting), editable in the editor's **Advanced** screen |
| `content/text/<language>/` | `kit.ron` (System, moderation, appearance), `error.ron` (errors), `game.ron` (this game's content) |
| `assets/sprites/` | floors (`tiles/`), player and wolf (`creatures/`, strips of 4 frames × 4 directions) |
| `assets/icons/`, `assets/fonts/`, `assets/themes/`, `assets/text/` | icons, fonts, theme and interface texts |

Replace any image with yours under the same name. The game's name is `ui.game.title` in `assets/text/<language>/ui.ron`; the client reads it when it starts, so there is nothing to build.

## Art

The floor, player, wolf and potion art was generated with PixelLab and adjusted by hand. PixelLab's terms give it to the creator for commercial use, so it ships under the kit's MIT licence ([credits](assets/CREDITS.md)). Replace it freely.
