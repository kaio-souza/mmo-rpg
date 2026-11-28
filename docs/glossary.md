# Glossary

| Term | Meaning |
|---|---|
| **Kit** | This project: server, client, editor and tools for making MMOs as data |
| **Game folder** | A folder with `content/` and `assets/`; tools run from it ([concepts](concepts.md#game-folder)) |
| **Template** | `templates/new-game/`, the small playable game users copy to start |
| **Example** | Riftara, `examples/riftara/`: a full game made with the kit, also the tests' data |
| **Content** | The rules as RON files in `content/` ([content](content/README.md)) |
| **Asset** | An image, font, theme, sound or interface text in `assets/` ([assets](assets.md)) |
| **Id** | A content name in `[a-z0-9_]`, the file name; stable forever |
| **Text key** | The name of a text (`item.wolf_pelt.name`); each language gives its text |
| **Text pack** | All content texts of one language, sent by the server and cached by the client |
| **Tick** | One step of the simulation; 20 per second |
| **Permille** | Thousandths: `600` is 60%, `1600` is ×1.6 |
| **Tile** | One 64×32 diamond of the map; one creature per tile |
| **Anchor** | The pixel of an image that sits on the centre of its tile |
| **Strip** | One PNG holding an animation: frames side by side, one row per direction (S, W, N, E) |
| **Zone** | One map with its own simulation; zones side by side form the world |
| **Seam** | The border between two zones; each simulates a 24-tile band of the other |
| **Ghost** | A snapshot of an entity from the neighbouring zone, with no logic, so players see across a seam |
| **Region** | A rectangle with a rule: `Safe`, `Pvp` or `Workshop` |
| **Gateway** | The process that takes connections, TLS and login, owns the database and routes to zones |
| **Intention** | What the client asks for ("walk there"); the server may refuse |
| **Interest** | What one player receives: within the view radius and in line of sight |
| **Diff** | The changes since the last tick that one player receives |
| **Overload level** | N0 to N3: how much a late zone degrades ([runtime](runtime.md#overload)) |
| **Replay** | A zone's commands recorded tick by tick, re-run to the same state hashes |
| **Sheet** | A character's saved data (attributes, items, quests...), a versioned blob |
| **Escrow** | Items and coins held by the server for a recipient, saved atomically |
| **Companion** | A tamed monster that follows a player and fights beside them ([companions](mounts-and-companions.md)) |
| **Karma** | A character's hidden morality (-1000 to 1000), moved by dialogues; opens options and titles ([dialogues](content/npcs-and-quests.md)) |
| **Server goal** | One shared quest for the whole server over a counter, read with `/goals` ([server goals](content/characters.md#server-goals)) |
| **Dev mode** | `server --dev`: no TLS, login by name, localhost only |
| **Attributes** | STR, VIT, AGI, DEX, INT, SPI |
| **Vigor** | The stamina resource: weapon blows and techniques spend it |
| **Proficiency** | A skill from 0 to 100 that grows by use (sword, mining, light magic) |
| **Technique** | An active ability on the hotbar, unlocked by proficiency, levels 1 to 10 |
| **Class** | A path chosen at a level, tiers 1 to 4, giving points and faster growth |
| **Title** | A recorded deed that gives passive bonuses and can gate classes |
| **Special attribute** | A 0 to 100 attribute earned by titles or grown by a class (faith, light) |
| **Counter** | Per-character count of an event family (kills, quests), used by titles |
| **Affinity** | How much an NPC likes a player; dialogue can test and change it |
| **Node** | A resource on the map (ore vein, herb patch) that empties per player |
| **Purse** | What a shop pays one player per period |
| **Rarity** | Common, Rare, Epic, Unique, Legendary, Mythic |
| **Appraisal** | Revealing a piece's hidden attribute rolls |
| **Falling** | The state before death: a fallen character can be revived |
| **System** | The voice of the game in the chat log (notices, warnings) |
| **Signal** | A number measured in the world per window (monsters alive, deaths, treasury) that switches world quests on ([world quests](world-quests.md)) |
| **World quest** | A quest written in the editor that is offered only while its condition holds, paid from a funder's escrow |
| **Town** | A set of regions with a treasury, taxes, guards, laws and legitimacy ([towns and politics](towns-and-politics.md)) |
| **Legitimacy** | A town's standing with its people (0 to 1000); low legitimacy means tax evasion and fewer guards |
| **Masterwork** | A crafted piece made with a near-perfect minigame score; it carries a mark beside its maker's signature ([crafting](crafting-minigames.md)) |
| **Server project** | A collective work for a town, guild or kingdom; delivered items go to an armoury, not into the void ([projects](server-projects.md)) |
| **Armoury** | The items an organisation owns; its NPC guards wear them |
| **Chronicle** | The written history of a town or kingdom, entries made only by the engine |
| **Instance** | A zone created on demand for one party, never joined by seams ([dungeons](dungeons-and-raids.md)) |
| **Siege** | A fight for a claimable town in a fixed window ([kingdoms](kingdoms-and-nations.md)) |
| **Server first** | The first character of the server to earn a title marked `announce: First`; told to everyone and kept for good (`/firsts`) |
| **Unique** | A title or class only one character of the server holds at a time ([D60](decisions.md)) |
| **Heir** | The character who earns a unique title or class after its holder stopped playing for `heir_after_days` |
| **Ranking** | A public list of the best characters by level, counter, proficiency or titles (`rankings/`, `/ranking`) |
| **Status** | A timed effect a blow leaves: poison, stun or slow |
| **Phase** | A stage of a monster's fight from a share of HP down, with its own blows |
