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
| 6 | ✅ **Done: editor buttons** for "New game" and "Play" (they run `mmo`; [tools](tools.md#editor)) | a designer never opens a terminal |
| 7 | **Kit defaults as one package** (theme, fonts, interface icons, interface sounds, interface texts) that the template and the example share | defaults live in one place |
| 8 | **Full world measured**: 2000 bots with combat spread over the example's zones | the targets in [performance](performance.md) are measured |
| 9 | **`pack`**: grid and palette checks, cut CJK fonts, `assets.pak` | releases ship one checked file |
| 11 | **Browser client in releases**: `mmo publish` writes the web folder (page, `.wasm`, `game.bundle`) beside the desktop package. *The client already builds for the browser ([web](web.md))* | a game is playable from a link |
| 12 | **Touch and phones**: gestures, touch sizes in the theme, on-screen buttons, small-screen layout, steps T1 to T4 ([touch](touch.md)) | the template is playable on a phone in the browser |
| 10 | **Layered characters**: body, hair and clothes recoloured by palette keys. *Engine done (body, clothes and hair layers tinted by skin and hair colours); first art in review in `testes-arte/layers-v1/`. Next: [character parts](character-parts.md), a look built from parts (body, hair, clothes) each with its options and palette, a "Character" screen in the editor, and the MVP tops, trousers and shoes as options* | a character's look is combinations, not one sprite each |

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

### A living world

Quests and money that answer the state of the world, all edited in the editor ([world quests](world-quests.md), [towns and politics](towns-and-politics.md)).

| # | Deliverable | Done when |
|---|---|---|
| W1 | **Towns, treasury and taxes** in the gateway, with coin telemetry per flow | taxes reach a treasury the designer sees live |
| W2 | **Signals and world quests** (`Kill`, `Reach`, `Patrol` objectives; offer, escrow, withdraw) | the wolf quest appears when wolves crowd the trail and leaves when they are gone |
| W3 | **Guards and legitimacy** with tax evasion | an empty treasury pulls the town into the spiral and players can pull it out |
| W4 | **Laws, factions and events** | a designer turns a law or an event on live |
| W5 | **Policy table (P2)**, then **council (P3)**, then **between towns (P4)** | towns react on their own, then players govern within limits |
| W6 | **Crafting minigames** and masterworks ([crafting](crafting-minigames.md)) | a skilled player makes better pieces than a click, and the server proves it |
| W7 | **Server projects**, armouries and the chronicle ([projects](server-projects.md)) | guards wear swords players made, with their names |
| W8 | **Guilds** with ranks, bank and wars ([guilds](guilds.md)) | two guilds can fight a war with a score and a stake |
| W9 | **Instances, dungeons and raids** ([dungeons](dungeons-and-raids.md)) | a party clears its own copy of a dungeon and the record is kept |
| W10 | **Mounts and companions** ([mounts](mounts-and-companions.md)) | a player rides and keeps a named pet that grows |
| W11 | **NPC memory, ambition and roles** ([living NPCs](living-npcs.md)) | an NPC remembers what a player did and chases its own goal |
| W12 | **Claimable towns, sieges, kingdoms** ([kingdoms](kingdoms-and-nations.md)) | a guild takes a town in a siege and rules it within limits |
| W13 | **Player homes** with farming, breeding and carpentry ([homes](player-homes.md)); after G6 and W10 | a player plants, breeds and furnishes a home friends can visit |

### Genre mechanics

The engine work that opens the most [genre references](genre-references.md) at once.

| # | Deliverable | Done when |
|---|---|---|
Order by how many references each opens ([genre references](genre-references.md#what-repeats)): G2, G3, G4, then instances.

| # | Deliverable | Done when |
|---|---|---|
| G2 | ✅ **Done: announcements, server firsts, rankings** (`announce` on titles, `rankings/`, `/ranking`, `/firsts`) | the whole server sees "first to earn X" once |
| G3 | ✅ **Done: titles and classes unique per server**, with an heir ([D60](decisions.md)); items in G8 | only one player can hold it |
| G4 | ✅ **Done: statuses, area techniques, boss phases** ([D62](decisions.md)) | a boss changes pattern at half HP |
| G5 | ✅ **Done: instances, step 1: moving between zones without a seam** ([portals](content/world.md#zoneron)) (a portal tile or an NPC sends a player to a tile of another zone: leave, save, enter, client reset) | a portal in one zone takes a player to another and back |
| G6 | ✅ **Done: instances, step 2: zones made at runtime** ([instance](content/world.md#zoneron), [D63](decisions.md)) from a map marked `instance: true`, own seed, closed when empty; death and timeout send players back outside | a party enters its own copy of a crypt |
| G7 | ✅ **Done: instances, step 3: `dungeons/` content and its editor form** ([dungeons](content/world.md#dungeons), [D64](decisions.md)). *Next for dungeons: stages, raids, the finder ([dungeons](dungeons-and-raids.md))* (party size, level, time limit, entrance, rewards), lockouts per character, fastest clears as a ranking | a designer makes a dungeon in the editor and a party clears it |
| G8 | ✅ **Done: items unique per server** ([items](content/items-and-economy.md#items), [D65](decisions.md)) | a relic exists once |
| G9 | ✅ **Done: example in the style of The Gamer** (`examples/everyday-system/`), content only, with an instant dungeon | a player grows every attribute and skill by use and earns titles for it |
| G11 | ✅ **Done: karma** ([D67](decisions.md)): hidden, moved by dialogues, read by dialogue conditions and title triggers | an NPC refuses the cruel and a title greets the kind |
| G12 | ✅ **Done: server goals, first step** ([D68](decisions.md)): `goals/` over a counter, announced by quarters, `/goals`, `GoalDone` in dialogues. *Next: rewards, timers and repeats ([world quests](world-quests.md))* | the whole server hunts 500 wolves together |
| G10 | Then W10 companions (Taming Master, Solo Leveling), W6 crafting minigames and W8/W12 guilds and kingdoms (Overgeared, Moonlight Sculptor) | |

### Ideas (not scheduled)

Ideas to make a creator's life easier: sprites baked from voxel and 3D models, a sound generator, one-click translation with review, wizards, balance and map helpers ([creator tools](creator-tools.md)). Each becomes a numbered item when picked.

## Open questions

| Question | Blocks |
|---|---|
| ⚠ Open: one home per character or per account; visits while the owner is offline ([homes](player-homes.md#open)) | W13 |
| ⚠ Open: a virtual stick besides tap to move; portrait or landscape only ([touch](touch.md#open)) | T4 |
| ⚠ Open: check that overload levels N1 and N2 thin AI and far updates as designed ([runtime](runtime.md)) | crowded zones |
