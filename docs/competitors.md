# Competitors

Who competes with the kit for **creators** (other tools for making MMOs), and with games made with it for **players**. Facts marked ⚠ were not verified; sources at the end (checked 2026-11-27; the kit's own column updated the same day after taming, companions, karma, server goals, rank, bestiary and weather). Related: [genre references](genre-references.md), [roadmap](roadmap.md).

## Tools (the creator's choice)

| | **This kit** | **Intersect Engine** | **RPG Maker MZ + MMO plugin** | **uMMORPG (Unity)** | **Server emulators** (Canary, ModernUO, rAthena) |
|---|---|---|---|---|---|
| For | Designers who do not program, 2D isometric MMOs | 2D MMOs without programming | RPG Maker users | Unity programmers | A server for an existing game |
| Language | Rust | C# (MonoGame) | JavaScript | C# (Unity) | C++ / C# |
| Licence | MIT, everything | Editor and server GPLv3, client MIT; games may be sold | RPG Maker paid; plugins free to ~45 USD | 80 USD per edition, add-ons extra | Open, but client and art belong to another company |
| View | **Isometric** | Top-down tiles ⚠ | Top-down | 2D or 3D | The original game's |
| No-code editor | Forms from the content types, checks as you type, maps, dialogue graphs, themes, sounds | Mature, with events | RPG Maker's; online is an add-on | Unity's, with code | Files and scripts |
| Browser | Yes (WebAssembly) | No ⚠ | Yes | Unity web build | No |
| Server | Authoritative and **deterministic** with replays; zones over processes, seamless world | Single server ⚠ | Depends on the plugin; several in development | Single server (reviews) | Mature, one process |
| Item duplication | Items with identity, never cloned; atomic saves | ⚠ | Weak | ⚠ | Varies |
| Languages | 10, keyed texts, plural rules | ⚠ | Plugins | Your own | Varies |
| Ready mechanics | Skills by use, classes (rare ones opened by a title), titles with bonuses and a last stand, rarity and appraisal, signed crafting, five quest objectives, adventurer rank, groups, trade, instances and dungeons, boss phases, rankings, unique items, **taming and companions** (requirements the designer picks), bestiary, hidden karma, **server goals**, rest, barbers, moderation | Combat, event quests, crafting, groups, guilds ⚠ | RPG Maker's turn-based combat | Many systems, some as paid add-ons | The original game's, complete |
| World rules | Day, night and weather (per zone, the same for everyone) open dialogues and bring or take monsters, all set in the editor |  Day and night ⚠ | Day and night by plugins | Your own code | The original game's |
| Scripting | None, on purpose: closed blocks ([D5](decisions.md)) | Visual events | JavaScript | C# | Lua, C#… |
| Mobile | Browser; touch planned (landscape, tap to move and a floating stick, [touch](touch.md)) | ⚠ | Web | Unity builds | Original client |
| Maturity | New: no showcase game, no community | Years, forum, published games | Huge community | One developer, many buyers | Decades |

**Where the kit wins:** a server that does not lose or duplicate items and replays any bug; real isometric view; an editor that cannot break the game; genre mechanics built in (unique classes and relics, server firsts, taming, karma, server goals), each tuned by data instead of code; MIT and the browser.

**Where it loses:** community and maturity; no scripting (a missing mechanic waits for the engine); no showcase game or default art pack (new windows still use generic icons); no touch controls yet; no player market, mail, bank or guilds; changing the engine needs Rust.

**To compete with Intersect, the direct rival:** prebuilt binaries and the web client in releases (roadmap 3 and 11), a showcase game playable from a link, a larger default art pack, and more condition/effect hooks to cover what events give, without scripts.

## Games (the player's choice)

| Game | Why it competes | What it has that a kit game lacks |
|---|---|---|
| Old School RuneScape | The reference for long skill progression | Content, community, mobile |
| RPG MO (mo.ee) | Browser MMO of skills in tiles | Years of content, many gathering skills, player market |
| Tibia and private servers | 2D tiles, dangerous world, deaths that cost | Decades of content, a large private-server scene |
| Ultima Online free shards | Isometric sandbox with skills by use | Houses, player economy, PvP culture |
| Haven & Hearth | Near-isometric sandbox: build, farm, survive | Building and farming, a world players change |
| Albion Online | Modern isometric sandbox on PC and mobile | Studio production, regional markets, territory |
| Margonem | Big 2D browser MMO in Poland | Years of content, regional base |
| Rucoy Online and similar | Pocket 2D MMOs | Made for touch screens |
| Brighter Shores | New skills-and-quests game by RuneScape's creator | His name, fresh content |
| Ragnarok Online private servers | 2D/2.5D class nostalgia | Known classes, community |
| Dofus / Wakfu | Isometric 2D with strong art | Turn-based tactics: another audience |

None combines an open modern engine, a server that never duplicates items, a seamless world, browser play, many languages and a no-code editor. What every one of them has and a kit game lacks is **content and community**.

## What would win players over

For players of sandbox and skilling MMOs (UO, mo.ee), in order: a player market and mail (escrow exists), chat channels, farming and more gathering nodes, a bank, guilds (W8), touch controls, and above all one large game made with the kit. Pets are no longer missing: taming and companions exist, and an army raised from the fallen is [planned](necromancy.md); what pet players still miss is pets that grow. A UO-style spiritual successor (own world and names) fits the kit best: isometric view, skills by use, signed crafting; it needs [player homes](player-homes.md), a total skill cap, reputation and player vendors.

## Sources

- Intersect: [licence](https://github.com/AscensionGameDev/Intersect-Engine/blob/main/LICENSE.md), [Engines Database](https://enginesdatabase.com/engine/intersect), [ModDB](https://www.moddb.com/engines/intersect-engine/articles)
- uMMORPG: [Definitive Edition](https://assetstore.unity.com/packages/templates/systems/ummorpg-definitive-edition-mmorpg-engine-399784), [reviews](https://assetstore.unity.com/packages/templates/systems/ummorpg-51212/reviews), [2D Remastered](https://assetstore.unity.com/packages/tools/network/ummorpg-2d-remastered-mmorpg-engine-102632)
- RPG Maker MZ: [MMORPG Maker](https://rmalizia44.itch.io/mmorpg-maker), [Falcon MMO MZ](https://decinbr.itch.io/falcon-mmo-mz), [MMORPGMaker MZ](https://github.com/AugustoMegener/MMORPGMaker-MZ), [Online Multiplayer](https://qkingsoftware.itch.io/rpgmakermp)
- Games: from the team's knowledge, not checked for this page.
