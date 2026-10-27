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
