# Tools

Every tool runs from the game folder and takes `--help`. `mmo` runs the others for you.

## `mmo`

One command for a game. It runs the kit's other tools, which must sit in the same folder (`target/release` after `cargo build --release`), so it needs no flags to remember.

| Command | Does |
|---|---|
| `mmo new <folder> [--kit DIR]` | copies the new-game template to a new folder (`--kit` when `mmo` does not run from the kit) |
| `mmo edit` | the editor on this game |
| `mmo check` | `validate content --assets assets` |
| `mmo play [--name NAME]` | `server --dev`, then `client --dev` on it (the login screen; `--name` enters directly as that character); the server stops with the client |
| `mmo publish [--out DIR]` | checks the game, then copies `content/`, `assets/`, `server` and `client` to `dist/` (or `DIR`); it refuses to overwrite a folder |

## `editor`

A local web page (`http://127.0.0.1:7878`, local only; the version for a published game is planned in the [roadmap](roadmap.md#the-editor-on-a-published-game)) where a designer or artist makes the whole game without opening a file. `--content`, `--assets` and `--port` change the defaults.

| Screen | Does |
|---|---|
| Categories | items, NPCs, monsters, quests, dialogues, techniques, classes, titles, shops, recipes, nodes, maps; **Advanced** for formulas, counters, zone files and moderation reasons |
| Create | starts from a **name** (the id comes from it), empty or as a copy of an existing one |
| Form | generated from the content type; references are pick lists, never typed ids; errors show on the field half a second later |
| Map | brush, bucket, rectangle, line, eyedropper; NPCs, monsters, nodes, regions, spawn; undo; a world view to place zones side by side, with the seam band |
| Dialogue | a map of steps, conditions and effects written as sentences, a clickable preview |
| Texts | key × language table with "missing" marks; **Languages** manages `locales.ron` |
| Graphics, Floors and scenery | sprites and icons with previews, PNG upload, `tiles.ron` and `sprites.ron` |
| Theme, Sounds, Game | interface theme; sound files and bindings; game name and login background |
| Economy, Moderation, Replays | the `admin` reports and actions with buttons; a recorded zone seen from above with a timeline |

Save in the editor with a `--dev` server running and the change is in the game within seconds.

## `validate`

```sh
validate content --assets assets      # content of a game
validate content --allow-missing-text  # English only is enough
validate arch                         # crate dependencies against arch.ron
```

Prints `content ok: N text keys in M locales, ...` or the list of problems, then warnings. CI validates the template and the example.

## `admin`

Needs access to the database (`--db sqlite:data/game.db`, `--content`). Moderation: `show`, `promote`, `demote`, `mute`, `suspend`, `ban`, `ip-ban`, `revoke`, `log`, `reset-password`, `rename`, `restore` (a deleted character). Operation: `maintenance <time|cancel>`, `verify-backup <file>`. Reports: `report economy`, `suspects`, `reports` (chat reports), `failed-logins`.

## `bots`

Load tests: N connections with a behaviour, then a report (round trip, drops, crossings).

```sh
bots --count 1000 --behavior walk --seconds 60
bots --count 20 --behavior trade --seconds 600   # hunt, sell loot, repair, buy
bots --count 200 --behavior seam --seconds 300   # cross between two zones
```

Behaviours: `idle`, `walk`, `trade`, `seam`.

## `replay`

Re-runs a zone recording and compares the state hash of every tick ([runtime](runtime.md#crashes-and-replays)).

## `pack`

Planned: check art against the grid and palette, cut CJK fonts to the characters in use, and build `assets.pak` for release. Today it is a stub; releases ship the `assets/` folder.
