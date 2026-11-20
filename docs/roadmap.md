# Roadmap

## Where it stands

The engine runs a complete game (the Riftara example: seven zones, combat, classes, titles, crafting, shops, groups, trade, moderation, ten languages) and a small one (the template). The work now is making it easy for others to build their own.

## Next

| # | Deliverable | Done when |
|---|---|---|
| 1 | ✅ **Done: `mmo` command**: `mmo new <folder>`, `mmo edit`, `mmo play` (server and client together), `mmo check`, `mmo publish` | a new game is one command away and runs with one more |
| 2 | ✅ **Done: one client for any game**: interface texts and game title loaded from the game's `assets/` at runtime | no rebuild per game; `MMO_GAME_DIR` gone |
| 3 | **Prebuilt binaries** for Linux and Windows in releases, with the template | users need no Rust |
| 4 | **Generic release workflow**: package any game folder (server with `content/`, client with `assets/`) | publishing a game is one command |
| 5 | **Content format version** and a migration path between kit versions | updating the kit never silently breaks a game |
| 6 | **Editor buttons** for "New game" and "Play" | a designer never opens a terminal |
| 7 | **Kit defaults as one package** (theme, fonts, interface icons, interface sounds, interface texts) that the template and the example share | defaults live in one place |
| 8 | **Full world measured**: 2000 bots with combat spread over the example's zones | the targets in [performance](performance.md) are measured |
| 9 | **`pack`**: grid and palette checks, cut CJK fonts, `assets.pak` | releases ship one checked file |
| 10 | **Layered characters**: body, hair and clothes recoloured by palette keys | a character's look is combinations, not one sprite each |

### The editor on a published game

A designer who does not program manages the live game from the editor: content (quests, items, NPCs, maps), moderation, economy, maintenance. No terminal, no SSH, no files.

| # | Deliverable | Done when |
|---|---|---|
| E1 | **Editor on the server's private network only**: it listens on the VPN address (WireGuard), never on the public IP; the firewall keeps only the game port open to the internet. `ops/install.sh` sets up the VPN and the editor service, and `admin vpn-add <name>` makes a config file (or QR code) for a team member's WireGuard app ([private network](operations.md#private-network-planned)) | the designer installs one app once, then opens a link |
| E2 | **Login with the game's accounts and roles** (a second layer behind the VPN): `admin` (everything), `designer` (content), `moderator` (moderation and reports); the same brute-force defence as the game; every action in `admin_log` | each person sees only their screens |
| E3 | **Draft and publish**: edits go to a draft that is checked as you type; **Publish** applies it to the running game in seconds (hot reload); a plain-language list of what changes | changing a quest never needs a restart or a file |
| E4 | **History and restore**: every publish is a version with who, when and what; **Restore** puts an older one back | a mistake is undone with one click |
| E5 | **Plain-language guards**: content in use cannot be deleted (it is retired instead); a map that grows asks to **schedule a restart** (maintenance with countdown) | the designer never breaks the live game |
| E6 | **Live art and sound**: images and sounds uploaded in the editor reach players' clients without a new client release | a new monster sprite shows up for everyone |
| E7 | **Test before publishing**: a test world (same content draft, separate database) the designer can enter from the editor | big changes are tried before players see them |

After these, mechanics from the [possible list](mechanics.md#possible-mechanics-not-in-the-engine) enter one at a time, each as a generic engine type plus its editor form.

## Open questions

| Question | Blocks |
|---|---|
| ⚠ Open: how players' clients receive new art and sound from a live server (download on login, by content hash, like the text pack) | E6 |
| ⚠ Open: where the live content lives on the server (outside the release folder, e.g. `/var/lib/game/content`) and how a developer's local copy syncs with it | E3 |
| ⚠ Open: which of the engine's fixed choices become configuration (tile size, tick, directions, number of bodies) | step 5 |
| ⚠ Open: interface sounds in the template | nothing (the client plays nothing without them) |
| ⚠ Open: Postgres next to SQLite | servers that outgrow one machine's database |
| ⚠ Open: acting across seams, a gate queue between two sides, groups and trades across zones | dense borders between zones |
| ⚠ Open: queue players when a zone is overloaded instead of refusing them | crowded zones |
| ⚠ Open: second factor (TOTP) at login | nothing |
| ⚠ Open: bot telemetry for repeated routes and reaction time without tick cost | nothing (kills per hour already used) |
