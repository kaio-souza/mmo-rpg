# NPCs, dialogues and quests

Examples to copy: [`examples/riftara/content/npcs/`](../../examples/riftara/content/npcs) and [`quests/`](../../examples/riftara/content/quests).

## NPCs

`npcs/<id>.ron`: `(sprite: "guide", dialogue: "guide")`. Optional `shop: Some("<shop id>")` (right-click opens it). The name is the text key `npc.<id>.name`. Place NPCs with a zone's `npcs.ron` ([world](world.md#spawns)).

## Dialogues

`npcs/dialogues/<id>.ron` is a graph of steps; each step is a text and a list of options.

```ron
(
    start: "greeting",
    nodes: {
        "greeting": (
            text: "npc.guide.greeting",
            options: [
                (text: "npc.guide.option.work", next: "offer",
                 when: Quest(quest: "first_steps", state: NotStarted)),
                (text: "npc.guide.option.done", next: "thanks",
                 when: QuestReady("first_steps"),
                 effects: [CompleteQuest("first_steps")]),
                (text: "npc.guide.option.bye"),            // no `next`: ends the talk
            ],
        ),
    },
)
```

An option shows only when its `when` holds; choosing it runs its `effects` in order, then goes to `next`. A step has at most 9 options (keys 1 to 9).

| Conditions (`when`) | Effects |
|---|---|
| `Item(item, qty)` holds at least | `GiveItem(item, qty)`, `TakeItem(item, qty)` |
| `Coins(n)` | `GiveCoins(n)`, `TakeCoins(n)` |
| `Quest(quest, state: NotStarted / Active / Completed)` | `StartQuest(id)`, `CompleteQuest(id)` |
| `QuestReady(id)` objectives done | `TopUpItem(item, qty)` gives only what is missing |
| `Affinity(n)` with this NPC | `Affinity(n)` adds to it |
| `RecipeKnown(id)`, `TechniqueKnown(id)` | `LearnRecipe(id)`, `LearnTechnique(id)` |
| | `Teleport((x: 300, y: 12))`: to a tile of the world; ends the dialogue |
| `All([...])`, `Any([...])`, `Not(...)` | |

The editor shows the dialogue as a map of steps, writes conditions and effects as sentences, and has a preview you click through.

## Quests

```ron
(
    rank: F,                       // F, E, D, C, B, A, S, SS, SSS
    repeatable: false,             // with `cooldown_secs` when true
    objectives: [Talk(npc: "herbalist"), Item(item: "glow_herb", qty: 3)],
    rewards: [Coins(150), Item(item: "travel_bread", qty: 2), Technique("kick")],
)
```

- Objectives: `Talk(npc)` while the quest is active, `Item(item, qty)` held (taken on completion), `Kill(monster, qty)` defeated while active (by everyone who earns its XP), `Reach(pos: (x, y), radius: 2)` stood on (travel; `pos` must be walkable ground of a zone), `Craft(item, qty)` crafted while active. The quest pointer leads to the next one: the NPC, the monsters, the place.
- Rewards: `Item`, `Coins`, `Technique` (taught at level 1).
- Quests start and complete through dialogue effects. The client shows the quest log, NPC markers and a direction to the next goal on the tracker and minimap.
- Texts: `quest.<id>.name`, `quest.<id>.desc`.

## Emotes

`emotes/<id>.ron`: `(cooldown_ticks: 40, icon: "emote_wave")`. Players use `/emote <id>`; the icon shows in a bubble over the character.

## Moderation reasons

`moderation/reasons.ron` lists the reason keys a moderator picks when muting, suspending or banning: `["moderation.reason.cheating", ...]`, each with text in every language.
