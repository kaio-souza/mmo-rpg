# The client

What players get with every game made with the kit.

## Screens

1. **Login:** the game's title large over the account and password, register; the language list in the top right corner (each language in its own name), the client's version in the bottom left. The server field shows only when the player has one to choose (no `--server`, and not exactly one server in `login.ron`). A background image comes from `assets/ui/login_background.png`; music from `sounds.ron` (`screens: { "login": ... }`).
2. **Queue:** when the world is full, position and estimated wait.
3. **Characters:** choose, create (name and appearance from `appearance.ron`, with a preview drawn as the game draws the character), delete (typing the name again; the name becomes free).
4. **Game:** the world with the HUD over it.

`client --dev --name <name>` skips the first three in development.

## Controls

| Default key | Action |
|---|---|
| Click, or `W` `A` `S` `D` | walk; `Space` stops |
| Mouse wheel | zoom (limited to the terrain received) |
| `Tab` / `Esc` | target the nearest monster / clear target, close windows |
| `F` | talk to the nearest NPC |
| `1`–`0` | hotbar: techniques and consumables, arranged by dragging |
| `I` `C` `K` `L` `M` `O` `G` | inventory, sheet, techniques, quests, map, social, crafting |
| `Enter` | chat |
| `F1` | help: keys and tips |
| `F9` / `F10` | mute all sound / options (volume per channel, test sound) |
| `Alt` | show names |
| `F12` | screenshot, saved as PNG in the user folder's `screenshots/` |

Clicking a creature's body or its feet picks it: left click attacks a monster or talks to an NPC, right click opens an NPC's shop or a player's menu (whisper, trade, invite, revive). `Tab` only selects a target; techniques on the hotbar act on it. Every key can be changed in `keybinds.ron` in the user folder (`~/.config/<game>/` on Linux, `%APPDATA%\<game>\` on Windows); a missing file means the defaults. `<game>` is the game's English title (`ui.game.title`) in lower case, with anything but letters, digits, `-` and `_` turned into `_`, so two games on one computer keep separate settings.

## Chat commands

| Command | Who | Does |
|---|---|---|
| `/w NAME TEXT` | everyone | whisper |
| `/report NAME` | everyone | report a player's recent chat to moderators |
| `/emote NAME` or `/e NAME`; `/emote` lists them | everyone | play an emote |
| `/invite NAME`, `/accept`, `/decline`, `/leave`, `/partykick NAME`, `/lead NAME` | everyone | groups (up to 6) |
| `/trade NAME`, `/offer SLOT QTY ... [coins N]`, `/confirm`, `/cancel` | everyone | direct trade (also through the trade window) |
| `/mute NAME 1h REASON`, `/suspend NAME 3d REASON`, `/ban NAME REASON`, `/kick NAME`, `/who NAME` | admins | moderation |
| `/maintenance 10m` or `cancel` | admins | announced restart |
| `/replay` | admins | every zone writes its recording to `replays/` |

## What the client keeps

- `login.ron` (mode `0600`): servers, last account, and this device's token per server and account. Never a password.
- `keybinds.ron`, sound options, language.
- Text packs cached by content hash in `<cache>/<game>/text/`, so they download only when content changes.

## Command line

`client --server HOST:PORT --account NAME [--register --character NAME] [--trust-cert FILE] [--change-password] [--locale CODE] [--assets DIR] [--theme ID] [--screenshot-after SECS]`, or `client --dev --server HOST:PORT --name NAME`. The password comes from `GAME_PASSWORD` or a prompt. `--screenshot-after SECS` takes one screenshot that many seconds after start, as F12 would (for documentation and bug reports).
