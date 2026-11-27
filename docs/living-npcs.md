# Living NPCs (sketch)

NPCs today have dialogue graphs, affinity per player and, planned, routines ([mechanics](mechanics.md)). This sketch adds three layers, all **closed blocks and data** ([D5](decisions.md)): no script, no language model deciding rules in the tick.

| Layer | Question it answers |
|---|---|
| **Memory** | What does this NPC remember about you and about the world? |
| **Ambition** | What does this NPC want, and what does it do about it? |
| **Role** | What position does it hold in a town, guild or kingdom, and what power comes with it? |

## Memory

A small, bounded list of **facts** per NPC, written by events:

```ron
// in npcs/<id>.ron
memory: (
    remembers: [
        (event: QuestDone, about: Player, weight: 3, forget_after_days: 60),
        (event: KilledNear(radius: 10), about: Player, weight: -5, forget_after_days: 30),
        (event: Delivered(project: Any), about: Player, weight: 2),
        (event: TownLaw(town: "aurelia"), about: World),
    ],
    capacity: 32,                       // oldest, lightest facts go first
),
```

- Facts are **typed records**, not text: `(event, subject, tick, weight)`.
- Conditions read them: `Condition::Remembers(event, at_least: 1)`; dialogue lines change ("You helped with the wall, I won't forget it").
- Gossip: an NPC may share facts with others in its town or faction (`spreads_to: Faction("merchants")`, a chance per day), so reputation travels.
- Memory lives in the gateway's database per (NPC, player); bounded, so 2000 players cost little.

## Ambition

One **goal** per NPC at a time, from a closed list, with steps the engine knows:

| Goal | What the NPC does | What players see |
|---|---|---|
| `Wealth(coins)` | raises prices, posts supply [world quests](world-quests.md) | a merchant expanding the shop |
| `Office(role)` | asks for support, gains backers | a candidate in a council |
| `Revenge(faction)` | funds bounties | hunts against a bandit faction |
| `Craft(masterwork)` | asks for rare materials | a smith's personal quest |
| `Protect(town)` | joins or funds a [project](server-projects.md) | a captain rallying help |

Progress is a number per goal, moved by events and by players (help or sabotage). When reached or failed, the NPC picks another goal from its list by weights. **Results are content**: a new shop tier, a title for helpers, a role change.

## Role

An NPC can hold a **role** in an organisation: `Mayor`, `Captain`, `Treasurer`, `Guildmaster`, `Lord`, `King`. A role has powers drawn from the [towns](towns-and-politics.md) policy table (raise a tax within limits, post a project, declare a bounty). An NPC in a role applies its **temperament** (`Cautious`, `Greedy`, `Just`, `Warlike`: weights over the policy rows). Roles can pass to players in [kingdoms](kingdoms-and-nations.md).

## Editor

- NPC form: Memory (events to remember), Ambition (goals with weights and outcomes), Role and temperament.
- An **NPC inspector** on the live game: current goal and progress, last facts, who it likes most.

## Engine work

1. `defs`: `memory`, `ambitions`, `role`, `temperament` in NPCs; new conditions and effects.
2. Gateway: facts table, goals state, gossip once per day (seeded).
3. `sim`: emit the events NPCs remember; NPCs only read their state.

## AI in the editor

A language model may help **in the editor only**, never in the running game: draft dialogue lines, NPC memory reactions, ambition texts, chronicle templates. What it writes is ordinary content: text keys with English plus **every active language** of the game (`locales.ron`).

- The editor translates the draft into all active languages in the same step, following the [translation](translation.md) rules (glossary, tone per language, placeholders).
- **Publish is refused** while any active language is missing for a key, as `validate` already does.
- The designer sees and can edit every language before publishing; the AI never publishes by itself.
- Nothing is generated at runtime, so the game stays deterministic and every line players read was reviewed.

## Heirs

A key NPC (a mayor, a master smith, a king) can **die** — in a siege, an event, a quest outcome — and be **replaced by an heir**, so the world changes without losing what it learned.

```ron
// in npcs/<id>.ron
mortal: Some((
    heirs: ["mayor_daughter", "mayor_nephew"],   // NPCs defined in content, in order
    inherits: (memory_permille: 500, role: true, goal: Keep),  // or NewGoal
    if_no_heir: Vacant,                          // the role opens (election, appointment)
)),
```

- The heir takes the **role**, a share of the **memory** (strongest facts first) and either the same goal or a new one; its temperament is its own.
- Players who helped or hurt the dead NPC are remembered by the heir at that share ("My father spoke of you").
- The death and succession go in the [chronicle](server-projects.md).
- Ids are forever ([D6](decisions.md)): the dead NPC is retired, not deleted, and its quests and dialogues stop being offered; quests already taken complete with the heir.
- Validation: heirs exist, are not themselves dead, and have the dialogue keys the role needs.

**Generations.** The chronicle keeps **every** succession forever (content only grows, [D6](decisions.md); an entry is a small row). Memory does not grow: each heir inherits from its direct predecessor only, so shares compound (50% of 50%…), facts under a weight floor are dropped, and the NPC's `capacity` still bounds it. The client shows the last three generations; the full line is one click away ([D42](decisions.md)).
