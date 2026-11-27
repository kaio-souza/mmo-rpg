# Content

Everything in `content/` is a RON file. The editor writes these files for you; this section is for reading them, reviewing a change, or editing by hand.

**Rule of thumb:** to make something new, open a file of the same kind in [`templates/new-game/content/`](../../templates/new-game/content) or [`examples/riftara/content/`](../../examples/riftara/content), copy it and change the values. The file in the repository is always the truth; the exact schema is in `shared/defs/`.

## Files

| Path | One file is | Page |
|---|---|---|
| `zones/<zone>/zone.ron`, `map.ron` | an area: where it sits, spawn, regions, the tile map | [world](world.md) |
| `zones/<zone>/monsters.ron`, `npcs.ron`, `nodes.ron` | what spawns in that area | [world](world.md) |
| `monsters/<id>.ron` | a monster | [world](world.md#monsters) |
| `classes/<id>.ron` | a class | [characters](characters.md#classes) |
| `proficiencies/<id>.ron` | a skill that grows by use | [characters](characters.md#proficiencies) |
| `techniques/<id>.ron` | an active ability | [characters](characters.md#techniques) |
| `titles/<id>.ron` | a title | [characters](characters.md#titles) |
| `special_attributes/<id>.ron` | an attribute earned by deeds or a class | [characters](characters.md#special-attributes) |
| `counters.ron` | the event families counted per character | [characters](characters.md#counters) |
| `rankings/<id>.ron` | a public ranking (`/ranking`) | [characters](characters.md#rankings) |
| `dungeons/<id>.ron` | the rules of an instance zone | [world](world.md#dungeons) |
| `appearance/appearance.ron` | bodies, skins, hair styles and colours offered at creation | [characters](characters.md#appearance) |
| `progression/formulas.ron`, `combat/formulas.ron` | level curve, points, death, day length; hit and crit numbers | [characters](characters.md#formulas) |
| `items/<id>.ron` | an item | [items and economy](items-and-economy.md) |
| `loot/<id>.ron` | a loot table shared by monsters or nodes | [items and economy](items-and-economy.md#loot) |
| `shops/<id>.ron`, `recipes/<id>.ron`, `nodes/<id>.ron` | a shop, a recipe, a resource node | [items and economy](items-and-economy.md) |
| `economy/formulas.ron` | prices, repair, durability, crafting and rarity numbers | [items and economy](items-and-economy.md#economy-numbers) |
| `npcs/<id>.ron`, `npcs/dialogues/<id>.ron` | an NPC and its dialogue | [NPCs and quests](npcs-and-quests.md) |
| `quests/<id>.ron` | a quest | [NPCs and quests](npcs-and-quests.md#quests) |
| `emotes/<id>.ron` | an emote | [NPCs and quests](npcs-and-quests.md#emotes) |
| `moderation/reasons.ron` | reasons a moderator can pick | [NPCs and quests](npcs-and-quests.md#moderation-reasons) |
| `text/locales.ron`, `text/<language>/<file>.ron` | languages and every text | [text](text.md) |

Any other file is an error. Every formula file and field is optional; a missing value keeps the engine default.

## Ids and names

- An id is the file name without `.ron`, in `[a-z0-9_]`. A zone's id is its folder name.
- References between files use ids: `loot: [(item: "wolf_pelt", chance: 600)]`.
- Names and descriptions are not written in these files: each id has text keys (`item.wolf_pelt.name`, `item.wolf_pelt.desc`) in `text/` ([text](text.md)).
- Images are called by name (`icon: "wolf_pelt"`, `sprite: "wolf"`); an empty name means "generic, no art" ([assets](../assets.md)).

## Numbers

- Time in ticks (20 per second): `cooldown_ticks: 600` is 30 s.
- Chances and multipliers in thousandths ("permille"): `chance: 600` is 60%, `damage_permille: 1600` is ×1.6.
- Money in copper; 100 copper make a silver and 100 silver a gold.

## Loading and checking

1. `defs` reads the folder and builds a catalog; any unknown field or file is an error.
2. Validation resolves every reference and checks ranges, maps, spawns and the texts of the required languages. Warnings do not block: an item with no source in the game (loot, shop, quest, dialogue or recipe), a text identical to English, a missing image.
3. The server loads only validated content. In `--dev` (or with `--watch-content`) it re-checks `content/` every 2 s and swaps it live; invalid content is refused and the old one stays. Deleting an item, quest or NPC is refused while the server runs.

## Growing content without hurting players

- Ids are forever: never rename or delete one players may hold. Retire things by removing their sources.
- A requirement is checked when earning (a class, a title), not for keeping it.
- New enum variants are appended at the end; files written for older versions keep working.
